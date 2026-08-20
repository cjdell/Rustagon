//! A scripted, in-memory implementation of [`Platform`] for headless tests.
//!
//! Every manager is a small fake:
//! - **Input/System/Hexpansion** — backed by [`EventQueue`]s; tests inject
//!   events with `push_*` / `set_*` helpers before each driver step.
//! - **Display** — records every `signal(LcdScreen)` with a logical frame
//!   counter (tests read it back with `screens()` / `last_screen()`).
//! - **Storage/Config** — in-memory (BTree-backed) file system and a
//!   `DeviceConfig` held in a mutex.
//! - **HTTP** — canned `Meta`/`Chunk`/`Done` responses matched by URL.
//! - **TCP** — a canned inbound event stream + an outbound byte recorder.
//! - **Power/Wifi/Led/Spawner/Ota** — trivial deterministic stand-ins.
//!
//! All shared state sits behind `embassy_sync::Mutex` acquired with
//! [`lock_spin`] (never `try_lock().unwrap()` — the e2e tests drive a
//! manager from a background menu thread while the test thread reads, and a
//! busy lock is normal). No guard is ever held across an `.await`, so the
//! spin never contends beyond a small synchronous section.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::str::from_utf8;
use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  mutex::{Mutex, MutexGuard},
  signal::Signal,
};

/// Blocking acquisition of a mock state mutex: spin on `try_lock` until it
/// succeeds. Test-only code — the guarded sections are short synchronous
/// bursts, and contention is only between the test thread and the background
/// menu thread (or between parallel test threads), so the spin is bounded.
/// Never use `try_lock().unwrap()` on these: a busy lock is normal there.
fn lock_spin<'a, T: Send + Sync>(m: &'a Mutex<CriticalSectionRawMutex, T>) -> MutexGuard<'a, CriticalSectionRawMutex, T> {
  loop {
    if let Ok(g) = m.try_lock() {
      return g;
    }
  }
}

use crate::platform::display::FRAME_BYTES;
use crate::platform::led::LedError;
use crate::platform::power::PowerStatus;
use crate::platform::storage::{ConfigFileTrait, StateError};
use crate::platform::{
  AppSpawner, ConfigHandle, DirEntry, DisplayError, DisplayHandle, DisplayManager, FileType, FsError, HexpansionHandle, HexpansionManager,
  HttpClient, HttpClientHandle, InputHandle, InputManager, LedHandle, LedManager, LocalFsTrait, PowerHandle, PowerManager, SpawnerHandle,
  StorageHandle, SystemHandle, SystemManager, TcpClient, TcpEvent, TcpHandle, TcpSession, TcpSessionBackend, WiFiHandle, WiFiManager,
  WifiStatus,
};
use crate::protocol::{HttpEvent, HttpRequest, HttpResponseMeta};
use crate::types::{
  DeviceConfig, DeviceEvent, HexButton, HexpansionEvent, HexpansionInfo, LedRequest, OtaError, SystemMessage, WifiDesiredState, WifiResult,
};
use crate::utils::EventQueue;

// ===========================================================================
// Display (recording)
// ===========================================================================

/// `(frames, next_frame_index)` — the recording display's log.
type FrameLog = (Vec<(u64, crate::types::LcdScreen)>, u64);

pub struct MockDisplay {
  state: Arc<Mutex<CriticalSectionRawMutex, FrameLog>>,
}

impl MockDisplay {
  pub fn new() -> Self {
    Self {
      state: Arc::new(Mutex::new((Vec::new(), 0))),
    }
  }

  // `lock_spin`, never `try_lock().unwrap()`: the menu task thread signals
  // and the test thread reads concurrently, and a busy lock is normal.
  pub fn clear(&self) {
    let mut g = lock_spin(&self.state);
    g.0.clear();
    g.1 = 0;
  }

  /// Every recorded `(frame_index, screen)` pair, in order.
  pub fn screens(&self) -> Vec<(u64, crate::types::LcdScreen)> {
    lock_spin(&self.state).0.clone()
  }

  pub fn last_screen(&self) -> Option<crate::types::LcdScreen> {
    lock_spin(&self.state).0.last().map(|(_, s)| s.clone())
  }
}

impl Default for MockDisplay {
  fn default() -> Self {
    Self::new()
  }
}

impl DisplayManager for MockDisplay {
  fn signal(&self, screen: crate::types::LcdScreen) -> Result<(), DisplayError> {
    let mut g = lock_spin(&self.state);
    let ts = g.1;
    g.0.push((ts, screen));
    g.1 = ts + 1;
    Ok(())
  }
  fn try_signal(&self, screen: crate::types::LcdScreen) -> Result<(), DisplayError> {
    self.signal(screen)
  }
  fn frame_buffer(&self) -> Option<&[u8]> {
    None
  }
  fn signal_raw_frame(&self, buffer: &[u8]) -> Result<(), DisplayError> {
    if buffer.len() != FRAME_BYTES {
      return Err(DisplayError::InvalidFrame);
    }
    Ok(())
  }
}

impl fmt::Debug for MockDisplay {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockDisplay").field("frames", &self.screens().len()).finish()
  }
}

// ===========================================================================
// LED (recording)
// ===========================================================================

pub struct MockLed {
  requests: Arc<Mutex<CriticalSectionRawMutex, Vec<LedRequest>>>,
}

impl MockLed {
  pub fn new() -> Self {
    Self {
      requests: Arc::new(Mutex::new(Vec::new())),
    }
  }
  pub fn requests(&self) -> Vec<LedRequest> {
    lock_spin(&self.requests).clone()
  }
}

impl Default for MockLed {
  fn default() -> Self {
    Self::new()
  }
}

impl LedManager for MockLed {
  fn request(&self, request: LedRequest) -> Result<(), LedError> {
    lock_spin(&self.requests).push(request);
    Ok(())
  }
}

impl fmt::Debug for MockLed {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockLed").finish()
  }
}

// ===========================================================================
// Power
// ===========================================================================

pub struct MockPower {
  status: Arc<Mutex<CriticalSectionRawMutex, PowerStatus>>,
  changed: Arc<Signal<CriticalSectionRawMutex, ()>>,
  pub power_offs: Arc<Mutex<CriticalSectionRawMutex, u32>>,
}

impl MockPower {
  pub fn new(initial: PowerStatus) -> Self {
    Self {
      status: Arc::new(Mutex::new(initial)),
      changed: Arc::new(Signal::new()),
      power_offs: Arc::new(Mutex::new(0)),
    }
  }
  pub fn set_status(&self, s: PowerStatus) {
    *lock_spin(&self.status) = s;
    self.changed.signal(());
  }
  pub fn get_status_sync(&self) -> PowerStatus {
    *lock_spin(&self.status)
  }
}

impl PowerManager for MockPower {
  fn power_off(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    let n = self.power_offs.clone();
    Box::pin(async move {
      *lock_spin(&n) += 1;
    })
  }
  fn get_status(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>> {
    let s = self.status.clone();
    Box::pin(async move { *lock_spin(&s) })
  }
  fn wait_for_change(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>> {
    let changed = self.changed.clone();
    let s = self.status.clone();
    Box::pin(async move {
      changed.wait().await;
      *lock_spin(&s)
    })
  }
}

impl fmt::Debug for MockPower {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockPower").finish()
  }
}

// ===========================================================================
// WiFi
// ===========================================================================

pub struct MockWifi {
  status: Arc<Mutex<CriticalSectionRawMutex, WifiStatus>>,
  changed: Arc<Signal<CriticalSectionRawMutex, ()>>,
  scan: Arc<Mutex<CriticalSectionRawMutex, Vec<WifiResult>>>,
  pub desired: Arc<Mutex<CriticalSectionRawMutex, Vec<WifiDesiredState>>>,
}

impl MockWifi {
  pub fn new(initial: WifiStatus) -> Self {
    Self {
      status: Arc::new(Mutex::new(initial)),
      changed: Arc::new(Signal::new()),
      scan: Arc::new(Mutex::new(Vec::new())),
      desired: Arc::new(Mutex::new(Vec::new())),
    }
  }
  pub fn set_status(&self, s: WifiStatus) {
    *lock_spin(&self.status) = s;
    self.changed.signal(());
  }
  pub fn set_scan_result(&self, networks: Vec<WifiResult>) {
    *lock_spin(&self.scan) = networks;
  }
  pub fn get_status_sync(&self) -> WifiStatus {
    lock_spin(&self.status).clone()
  }
}

impl WiFiManager for MockWifi {
  fn get_status(&self) -> Pin<Box<dyn Future<Output = WifiStatus> + Send + '_>> {
    let s = self.status.clone();
    Box::pin(async move { lock_spin(&s).clone() })
  }
  fn wait_for_status_change(&self) -> Pin<Box<dyn Future<Output = WifiStatus> + Send + '_>> {
    let changed = self.changed.clone();
    let s = self.status.clone();
    Box::pin(async move {
      changed.wait().await;
      lock_spin(&s).clone()
    })
  }
  fn set_desired_state(&self, state: WifiDesiredState) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    let d = self.desired.clone();
    Box::pin(async move {
      lock_spin(&d).push(state);
    })
  }
  fn scan(&self) -> Pin<Box<dyn Future<Output = Result<Vec<WifiResult>, ()>> + Send + '_>> {
    let s = self.scan.clone();
    Box::pin(async move { Ok(lock_spin(&s).clone()) })
  }
}

impl fmt::Debug for MockWifi {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockWifi").finish()
  }
}

// ===========================================================================
// Input
// ===========================================================================

pub struct MockInput {
  pub buttons: EventQueue<HexButton, 64>,
}

impl MockInput {
  pub fn new() -> Self {
    Self {
      buttons: EventQueue::new(),
    }
  }
  /// Push a discrete tap: a press plus its matching release — what a
  /// physical button press produces (the app-layer button repeater disarms
  /// on the release). For raw press/release edges (e.g. holding a button
  /// down to test repeat) use [`MockInput::push_button_event`].
  pub fn push_button(&self, b: HexButton) {
    self.push_button_event(b);
    if !b.is_released() {
      self.push_button_event(b.released());
    }
  }

  /// Push a single raw button event (one edge, no auto-release).
  pub fn push_button_event(&self, b: HexButton) {
    self.buttons.try_push(b);
  }
}

impl Default for MockInput {
  fn default() -> Self {
    Self::new()
  }
}

impl InputManager for MockInput {
  fn next_button(&self) -> Pin<Box<dyn Future<Output = HexButton> + Send + '_>> {
    let q = self.buttons.clone();
    Box::pin(async move { q.next().await })
  }
  fn inject_button(&self, button: HexButton) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    let q = self.buttons.clone();
    Box::pin(async move {
      q.push(button).await;
    })
  }
}

impl fmt::Debug for MockInput {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockInput").finish()
  }
}

// ===========================================================================
// System (boot button)
// ===========================================================================

pub struct MockSystem {
  pub events: EventQueue<SystemMessage, 16>,
}

impl MockSystem {
  pub fn new() -> Self {
    Self { events: EventQueue::new() }
  }
  pub fn push_boot(&self) {
    self.events.try_push(SystemMessage::BootButton);
  }
}

impl Default for MockSystem {
  fn default() -> Self {
    Self::new()
  }
}

impl SystemManager for MockSystem {
  fn next_button(&self) -> Pin<Box<dyn Future<Output = SystemMessage> + Send + '_>> {
    let q = self.events.clone();
    Box::pin(async move { q.next().await })
  }
  fn inject(&self, message: SystemMessage) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    let q = self.events.clone();
    Box::pin(async move {
      q.push(message).await;
    })
  }
}

impl fmt::Debug for MockSystem {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockSystem").finish()
  }
}

// ===========================================================================
// Hexpansion
// ===========================================================================

/// `(port, Some(info) | None)` for each hexpansion slot.
type HexpansionSlots = Vec<(u8, Option<HexpansionInfo>)>;

pub struct MockHexpansion {
  pub events: EventQueue<HexpansionEvent, 16>,
  pub device_events: EventQueue<DeviceEvent, 32>,
  state: Arc<Mutex<CriticalSectionRawMutex, HexpansionSlots>>,
}

impl MockHexpansion {
  pub fn new() -> Self {
    // Six hexpansion slots, all empty, mirroring the hardware.
    Self {
      events: EventQueue::new(),
      device_events: EventQueue::new(),
      state: Arc::new(Mutex::new((0..6).map(|p| (p, None)).collect())),
    }
  }
  pub fn set_state(&self, slots: Vec<(u8, Option<HexpansionInfo>)>) {
    *lock_spin(&self.state) = slots;
  }
  pub fn push_event(&self, ev: HexpansionEvent) {
    self.events.try_push(ev);
  }
  pub fn push_device_event(&self, ev: DeviceEvent) {
    self.device_events.try_push(ev);
  }
}

impl Default for MockHexpansion {
  fn default() -> Self {
    Self::new()
  }
}

impl HexpansionManager for MockHexpansion {
  fn next_event(&self) -> Pin<Box<dyn Future<Output = HexpansionEvent> + Send + '_>> {
    let q = self.events.clone();
    Box::pin(async move { q.next().await })
  }
  fn try_next_event(&self) -> Option<HexpansionEvent> {
    self.events.try_next()
  }
  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)> {
    lock_spin(&self.state).clone()
  }
  fn next_device_event(&self) -> Pin<Box<dyn Future<Output = DeviceEvent> + Send + '_>> {
    let q = self.device_events.clone();
    Box::pin(async move { q.next().await })
  }
  fn try_next_device_event(&self) -> Option<DeviceEvent> {
    self.device_events.try_next()
  }
}

impl fmt::Debug for MockHexpansion {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockHexpansion").finish()
  }
}

// ===========================================================================
// Storage (in-memory filesystem)
// ===========================================================================

#[derive(Default)]
struct Fs {
  files: BTreeMap<String, Vec<u8>>,
  dirs: BTreeSet<String>,
}

fn parent_of(p: &str) -> String {
  match p.rfind('/') {
    Some(i) => p[..i].to_string(),
    None => String::new(),
  }
}

fn entries_under(files: &BTreeMap<String, Vec<u8>>, dirs: &BTreeSet<String>, parent: &str) -> Vec<DirEntry> {
  let mut out = Vec::new();
  for name in files.keys() {
    if parent_of(name) == parent {
      out.push(DirEntry {
        name: name.clone(),
        file_type: FileType::File,
        size: files[name].len() as u32,
      });
    }
  }
  for name in dirs.iter() {
    if parent_of(name) == parent {
      out.push(DirEntry {
        name: name.clone(),
        file_type: FileType::Dir,
        size: 0,
      });
    }
  }
  out
}

pub struct MockStorage {
  fs: Arc<Mutex<CriticalSectionRawMutex, Fs>>,
}

impl MockStorage {
  pub fn new() -> Self {
    Self {
      fs: Arc::new(Mutex::new(Fs::default())),
    }
  }

  pub fn seed_file(&self, name: &str, contents: &[u8]) {
    let mut g = lock_spin(&self.fs);
    g.files.insert(name.to_string(), contents.to_vec());
  }

  pub fn read_sync(&self, name: &str) -> Option<Vec<u8>> {
    lock_spin(&self.fs).files.get(name).cloned()
  }

  pub fn clear(&self) {
    let mut g = lock_spin(&self.fs);
    g.files.clear();
    g.dirs.clear();
  }
}

impl Default for MockStorage {
  fn default() -> Self {
    Self::new()
  }
}

impl LocalFsTrait for MockStorage {
  fn format(&self) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let mut g = lock_spin(&fs);
      g.files.clear();
      g.dirs.clear();
      Ok(())
    })
  }
  fn list_files(&self) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let g = lock_spin(&fs);
      Ok(entries_under(&g.files, &g.dirs, ""))
    })
  }
  fn list_dir(&self, path: String) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let parent = path.trim_start_matches('/').to_string();
      let g = lock_spin(&fs);
      Ok(entries_under(&g.files, &g.dirs, &parent))
    })
  }
  fn get_file_size(&self, file_name: String) -> Pin<Box<dyn Future<Output = Result<u32, FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let g = lock_spin(&fs);
      g.files.get(&file_name).map(|f| f.len() as u32).ok_or(FsError::NotFound)
    })
  }
  fn read_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    size: u32,
  ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let g = lock_spin(&fs);
      let entry = g.files.get(&file_name).ok_or(FsError::NotFound)?;
      let start = (pos as usize).min(entry.len());
      let end = (start + size as usize).min(entry.len());
      Ok(entry[start..end].to_vec())
    })
  }
  fn write_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    buf: Vec<u8>,
    truncate: bool,
  ) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let mut g = lock_spin(&fs);
      let entry = g.files.entry(file_name).or_default();
      if truncate {
        entry.truncate(pos as usize);
      }
      let needed = (pos as usize) + buf.len();
      if needed > entry.len() {
        entry.resize(needed, 0);
      }
      entry[pos as usize..(pos as usize) + buf.len()].copy_from_slice(&buf);
      Ok(())
    })
  }
  fn read_text_file(&self, file_name: String) -> Pin<Box<dyn Future<Output = Result<String, FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let g = lock_spin(&fs);
      let entry = g.files.get(&file_name).ok_or(FsError::NotFound)?;
      Ok(from_utf8(entry).map_err(FsError::Decoding)?.to_string())
    })
  }
  fn write_text_file(&self, file_name: String, text: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let mut g = lock_spin(&fs);
      g.files.insert(file_name, text.into_bytes());
      Ok(())
    })
  }
  fn delete(&self, name: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let mut g = lock_spin(&fs);
      if g.files.remove(&name).is_some() || g.dirs.remove(&name) {
        Ok(())
      } else {
        Err(FsError::NotFound)
      }
    })
  }
  fn mkdir(&self, dir_name: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let mut g = lock_spin(&fs);
      g.dirs.insert(dir_name);
      Ok(())
    })
  }
  fn file_exists(&self, name: String) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let g = lock_spin(&fs);
      g.files.contains_key(&name) || g.dirs.contains(&name)
    })
  }
  fn get_file_type(&self, name: String) -> Pin<Box<dyn Future<Output = Result<FileType, FsError>> + Send + '_>> {
    let fs = self.fs.clone();
    Box::pin(async move {
      let g = lock_spin(&fs);
      if g.files.contains_key(&name) {
        Ok(FileType::File)
      } else if g.dirs.contains(&name) {
        Ok(FileType::Dir)
      } else {
        Err(FsError::NotFound)
      }
    })
  }
}

impl fmt::Debug for MockStorage {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockStorage")
      .field("files", &lock_spin(&self.fs).files.len())
      .finish()
  }
}

// ===========================================================================
// Config
// ===========================================================================

pub struct MockConfig {
  data: Arc<Mutex<CriticalSectionRawMutex, DeviceConfig>>,
}

impl MockConfig {
  pub fn new(initial: DeviceConfig) -> Self {
    Self {
      data: Arc::new(Mutex::new(initial)),
    }
  }
  pub fn get(&self) -> DeviceConfig {
    lock_spin(&self.data).clone()
  }
  pub fn set(&self, cfg: DeviceConfig) {
    *lock_spin(&self.data) = cfg;
  }
}

impl ConfigFileTrait<DeviceConfig> for MockConfig {
  fn get_json(&self) -> Pin<Box<dyn Future<Output = Result<String, StateError>> + Send + '_>> {
    let d = self.data.clone();
    Box::pin(async move {
      let g = lock_spin(&d);
      serde_json::to_string(&*g).map_err(|e| StateError::Error(e.to_string()))
    })
  }
  fn set_json(&self, json: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>> {
    let d = self.data.clone();
    Box::pin(async move {
      let parsed: DeviceConfig = serde_json::from_slice(&json).map_err(|e| StateError::Error(e.to_string()))?;
      *lock_spin(&d) = parsed;
      Ok(())
    })
  }
  fn get_data(&self) -> Pin<Box<dyn Future<Output = DeviceConfig> + Send + '_>> {
    let d = self.data.clone();
    Box::pin(async move { lock_spin(&d).clone() })
  }
  fn set_data(&self, new_state: DeviceConfig) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    let d = self.data.clone();
    Box::pin(async move {
      *lock_spin(&d) = new_state;
    })
  }
  fn save(&self) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }
}

impl fmt::Debug for MockConfig {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockConfig").finish()
  }
}

// ===========================================================================
// HTTP (canned responses)
// ===========================================================================

/// A canned HTTP response: meta + a series of body chunks.
#[derive(Clone, Debug)]
pub struct HttpScript {
  pub meta: HttpResponseMeta,
  pub chunks: Vec<Vec<u8>>,
}

pub struct MockHttp {
  scripts: Arc<Mutex<CriticalSectionRawMutex, Vec<(String, HttpScript)>>>,
  pub requests: Arc<Mutex<CriticalSectionRawMutex, Vec<String>>>,
}

impl MockHttp {
  pub fn new() -> Self {
    Self {
      scripts: Arc::new(Mutex::new(Vec::new())),
      requests: Arc::new(Mutex::new(Vec::new())),
    }
  }

  /// Register a canned response matched when the request URL contains `url`.
  pub fn script(&self, url: &str, meta: HttpResponseMeta, chunks: Vec<Vec<u8>>) {
    lock_spin(&self.scripts).push((url.to_string(), HttpScript { meta, chunks }));
  }

  /// Register a canned JSON body (status 200).
  pub fn json(&self, url: &str, body: &str) {
    let meta = HttpResponseMeta::new(200);
    self.script(url, meta, vec![body.as_bytes().to_vec()]);
  }

  pub fn requested_urls(&self) -> Vec<String> {
    lock_spin(&self.requests).clone()
  }
}

impl Default for MockHttp {
  fn default() -> Self {
    Self::new()
  }
}

impl HttpClient for MockHttp {
  fn request<'a>(&'a self, req: HttpRequest, channel: &'a crate::platform::HttpEventChannel) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    let scripts = self.scripts.clone();
    let requests = self.requests.clone();
    Box::pin(async move {
      lock_spin(&requests).push(req.url.clone());
      let url = req.url.clone();
      let matched = {
        let g = lock_spin(&scripts);
        g.iter()
          .find(|(u, _)| url.contains(u.as_str()))
          .map(|(_, s)| (s.meta.clone(), s.chunks.clone()))
      };
      match matched {
        Some((meta, chunks)) => {
          let _ = channel.send(HttpEvent::Meta(meta)).await;
          for chunk in chunks {
            let _ = channel.send(HttpEvent::Chunk(chunk)).await;
          }
          let _ = channel.send(HttpEvent::Done).await;
        }
        None => {
          let _ = channel.try_send(HttpEvent::Error);
        }
      }
    })
  }
}

impl fmt::Debug for MockHttp {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockHttp").finish()
  }
}

// ===========================================================================
// TCP (canned inbound stream + outbound recorder)
// ===========================================================================

#[derive(Default)]
struct TcpState {
  inbound: EventQueue<TcpEvent, 64>,
  outbound: Vec<Vec<u8>>,
  connect_ok: bool,
}

pub struct MockTcp {
  state: Arc<Mutex<CriticalSectionRawMutex, TcpState>>,
}

impl MockTcp {
  pub fn new() -> Self {
    Self {
      state: Arc::new(Mutex::new(TcpState::default())),
    }
  }
  pub fn set_connect_ok(&self, ok: bool) {
    lock_spin(&self.state).connect_ok = ok;
  }
  /// Queue an inbound event (in order).
  pub fn push_inbound(&self, ev: TcpEvent) {
    lock_spin(&self.state).inbound.try_push(ev);
  }
  pub fn inbound(&self) -> EventQueue<TcpEvent, 64> {
    lock_spin(&self.state).inbound.clone()
  }
  /// Everything the app has sent on the (fake) connection.
  pub fn outbound(&self) -> Vec<Vec<u8>> {
    lock_spin(&self.state).outbound.clone()
  }
}

impl Default for MockTcp {
  fn default() -> Self {
    Self::new()
  }
}

impl TcpClient for MockTcp {
  fn connect(&self, host: String, port: u16) -> Pin<Box<dyn Future<Output = Result<TcpSession, ()>> + 'static>> {
    let state = self.state.clone();
    Box::pin(async move {
      let ok = lock_spin(&state).connect_ok;
      if !ok {
        return Err(());
      }
      let session = TcpSession::new(Arc::new(MockTcpSession { state }));
      let _ = (host, port);
      Ok(session)
    })
  }
}

impl fmt::Debug for MockTcp {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockTcp").finish()
  }
}

pub struct MockTcpSession {
  state: Arc<Mutex<CriticalSectionRawMutex, TcpState>>,
}

impl MockTcpSession {
  fn inbound(&self) -> EventQueue<TcpEvent, 64> {
    lock_spin(&self.state).inbound.clone()
  }
}

impl TcpSessionBackend for MockTcpSession {
  fn next_event<'a>(&'a self) -> Pin<Box<dyn Future<Output = TcpEvent> + 'a>> {
    let q = self.inbound();
    Box::pin(async move { q.next().await })
  }
  fn try_next_event(&self) -> Option<TcpEvent> {
    self.inbound().try_next()
  }
  fn send<'a>(&'a self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    let state = self.state.clone();
    Box::pin(async move {
      lock_spin(&state).outbound.push(data);
    })
  }
  fn close<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    let state = self.state.clone();
    Box::pin(async move {
      lock_spin(&state).inbound.try_push(TcpEvent::Closed);
    })
  }
}

impl fmt::Debug for MockTcpSession {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockTcpSession").finish()
  }
}

// ===========================================================================
// Spawner (no-op)
// ===========================================================================

pub struct MockSpawner;

impl AppSpawner for MockSpawner {
  fn spawn(&self, _fut: Box<dyn Future<Output = ()> + Send + 'static>) {}
  fn spawn_local(&self, _fut: Box<dyn Future<Output = ()> + 'static>) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async {})
  }
}

impl fmt::Debug for MockSpawner {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockSpawner").finish()
  }
}

// ===========================================================================
// Platform
// ===========================================================================

/// The full scripted platform. Construct with [`MockPlatform::new`]; inject
/// input/events and read recorded state through the accessors.
pub struct MockPlatform {
  display: Arc<MockDisplay>,
  led: Arc<MockLed>,
  power: Arc<MockPower>,
  wifi: Arc<MockWifi>,
  input: Arc<MockInput>,
  system: Arc<MockSystem>,
  hexpansion: Arc<MockHexpansion>,
  http: Arc<MockHttp>,
  tcp: Arc<MockTcp>,
  storage: Arc<MockStorage>,
  config: Arc<MockConfig>,
  spawner: Arc<MockSpawner>,
  firmware_version: u32,
  entropy_counter: Arc<Mutex<CriticalSectionRawMutex, u64>>,
  pub resets: Arc<Mutex<CriticalSectionRawMutex, u32>>,
}

impl MockPlatform {
  /// Build a platform with a default config, offline wifi, and an empty
  /// 6-slot hexpansion state.
  pub fn new() -> Self {
    Self {
      display: Arc::new(MockDisplay::new()),
      led: Arc::new(MockLed::new()),
      power: Arc::new(MockPower::new(PowerStatus {
        vbat_mv: 4100,
        vsys_mv: 4100,
        vbus_mv: 0,
        charge_current_ma: 0,
        charge_voltage_mv: 4100,
        input_current_limit_ma: 0,
        is_charging: false,
        is_power_present: false,
        battery_fault: false,
      })),
      wifi: Arc::new(MockWifi::new(WifiStatus::Offline)),
      input: Arc::new(MockInput::new()),
      system: Arc::new(MockSystem::new()),
      hexpansion: Arc::new(MockHexpansion::new()),
      http: Arc::new(MockHttp::new()),
      tcp: Arc::new(MockTcp::new()),
      storage: Arc::new(MockStorage::new()),
      config: Arc::new(MockConfig::new(DeviceConfig::default())),
      spawner: Arc::new(MockSpawner),
      firmware_version: 1,
      entropy_counter: Arc::new(Mutex::new(0)),
      resets: Arc::new(Mutex::new(0)),
    }
  }

  // -- input / events --
  pub fn push_button(&self, b: HexButton) {
    self.input.push_button(b);
  }

  pub fn push_button_event(&self, b: HexButton) {
    self.input.push_button_event(b);
  }
  pub fn push_boot(&self) {
    self.system.push_boot();
  }
  pub fn push_hexpansion_event(&self, ev: HexpansionEvent) {
    self.hexpansion.push_event(ev);
  }
  pub fn push_device_event(&self, ev: DeviceEvent) {
    self.hexpansion.push_device_event(ev);
  }

  // -- display --
  pub fn screens(&self) -> Vec<(u64, crate::types::LcdScreen)> {
    self.display.screens()
  }
  pub fn last_screen(&self) -> Option<crate::types::LcdScreen> {
    self.display.last_screen()
  }
  pub fn clear_screens(&self) {
    self.display.clear();
  }

  // -- storage / config --
  pub fn seed_file(&self, name: &str, contents: &[u8]) {
    self.storage.seed_file(name, contents);
  }
  pub fn storage_read(&self, name: &str) -> Option<Vec<u8>> {
    self.storage.read_sync(name)
  }
  pub fn config(&self) -> DeviceConfig {
    self.config.get()
  }
  pub fn set_config(&self, cfg: DeviceConfig) {
    self.config.set(cfg);
  }

  // -- wifi / power --
  pub fn set_wifi_status(&self, s: WifiStatus) {
    self.wifi.set_status(s);
  }
  pub fn set_wifi_scan(&self, networks: Vec<WifiResult>) {
    self.wifi.set_scan_result(networks);
  }
  pub fn set_power(&self, s: PowerStatus) {
    self.power.set_status(s);
  }
  pub fn set_hexpansion_state(&self, slots: Vec<(u8, Option<HexpansionInfo>)>) {
    self.hexpansion.set_state(slots);
  }

  // -- http / tcp --
  pub fn http(&self) -> Arc<MockHttp> {
    self.http.clone()
  }
  pub fn tcp(&self) -> Arc<MockTcp> {
    self.tcp.clone()
  }
  pub fn tcp_connect_ok(&self, ok: bool) {
    self.tcp.set_connect_ok(ok);
  }

  // -- misc --
  pub fn power_off_count(&self) -> u32 {
    *lock_spin(&self.power.power_offs)
  }
  pub fn reset_count(&self) -> u32 {
    *lock_spin(&self.resets)
  }
}

impl Default for MockPlatform {
  fn default() -> Self {
    Self::new()
  }
}

impl Clone for MockPlatform {
  fn clone(&self) -> Self {
    Self {
      display: self.display.clone(),
      led: self.led.clone(),
      power: self.power.clone(),
      wifi: self.wifi.clone(),
      input: self.input.clone(),
      system: self.system.clone(),
      hexpansion: self.hexpansion.clone(),
      http: self.http.clone(),
      tcp: self.tcp.clone(),
      storage: self.storage.clone(),
      config: self.config.clone(),
      spawner: self.spawner.clone(),
      firmware_version: self.firmware_version,
      entropy_counter: self.entropy_counter.clone(),
      resets: self.resets.clone(),
    }
  }
}

impl fmt::Debug for MockPlatform {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockPlatform").finish()
  }
}

impl crate::platform::Platform for MockPlatform {
  fn display_manager(&self) -> DisplayHandle {
    DisplayHandle::new(Arc::new(ForwardDisplay(self.display.clone())))
  }
  fn led_manager(&self) -> LedHandle {
    LedHandle::new(Arc::new(ForwardLed(self.led.clone())))
  }
  fn power_manager(&self) -> PowerHandle {
    PowerHandle::new(Arc::new(ForwardPower(self.power.clone())))
  }
  fn wifi_manager(&self) -> WiFiHandle {
    WiFiHandle::new(Arc::new(ForwardWifi(self.wifi.clone())))
  }
  fn input_manager(&self) -> InputHandle {
    InputHandle::new(Arc::new(ForwardInput(self.input.clone())))
  }
  fn system_manager(&self) -> SystemHandle {
    SystemHandle::new(Arc::new(ForwardSystem(self.system.clone())))
  }
  fn hexpansion_manager(&self) -> HexpansionHandle {
    HexpansionHandle::new(Arc::new(ForwardHexpansion(self.hexpansion.clone())))
  }
  fn http_client(&self) -> Option<HttpClientHandle> {
    Some(HttpClientHandle::new(Arc::new(ForwardHttp(self.http.clone()))))
  }
  fn tcp_client(&self) -> Option<TcpHandle> {
    Some(TcpHandle::new(Arc::new(ForwardTcp(self.tcp.clone()))))
  }
  fn storage_manager(&self) -> StorageHandle {
    StorageHandle::new(Arc::new(ForwardStorage(self.storage.clone())))
  }
  fn config_manager(&self) -> ConfigHandle<DeviceConfig> {
    ConfigHandle::new(Arc::new(ForwardConfig(self.config.clone())))
  }
  fn spawner(&self) -> SpawnerHandle {
    SpawnerHandle::new(Arc::new(ForwardSpawner(self.spawner.clone())))
  }
  fn firmware_version(&self) -> u32 {
    self.firmware_version
  }
  fn entropy(&self, dest: &mut [u8]) {
    let mut base = *lock_spin(&self.entropy_counter);
    for b in dest.iter_mut() {
      *b = base as u8;
      base = base.wrapping_add(1);
    }
    *lock_spin(&self.entropy_counter) = base;
  }
  async fn format_storage(&self) -> Result<(), FsError> {
    self.storage.format().await
  }
  async fn software_reset(&self) {
    *lock_spin(&self.resets) += 1;
  }
  async fn ota_begin(&self) -> Result<u32, OtaError> {
    Ok(0)
  }
  async fn ota_write_chunk(&self, _offset: u32, _data: &[u8]) -> Result<(), OtaError> {
    Ok(())
  }
  async fn ota_commit(&self) -> Result<(), OtaError> {
    Ok(())
  }
}

// ===========================================================================
// Forwarders: thin `Arc<dyn Manager>` adapters over the concrete mocks, so the
// `Platform` impl can hand out cloneable handles without re-implementing each
// manager on `MockPlatform` itself.
// ===========================================================================

macro_rules! forward_debug {
  ($t:ident) => {
    impl fmt::Debug for $t {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(stringify!($t)).finish()
      }
    }
  };
}

struct ForwardDisplay(Arc<MockDisplay>);
impl DisplayManager for ForwardDisplay {
  fn signal(&self, s: crate::types::LcdScreen) -> Result<(), DisplayError> {
    self.0.signal(s)
  }
  fn try_signal(&self, s: crate::types::LcdScreen) -> Result<(), DisplayError> {
    self.0.try_signal(s)
  }
  fn frame_buffer(&self) -> Option<&[u8]> {
    self.0.frame_buffer()
  }
  fn signal_raw_frame(&self, b: &[u8]) -> Result<(), DisplayError> {
    self.0.signal_raw_frame(b)
  }
}
forward_debug!(ForwardDisplay);

struct ForwardLed(Arc<MockLed>);
impl LedManager for ForwardLed {
  fn request(&self, r: LedRequest) -> Result<(), LedError> {
    self.0.request(r)
  }
}
forward_debug!(ForwardLed);

struct ForwardPower(Arc<MockPower>);
impl PowerManager for ForwardPower {
  fn power_off(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    self.0.power_off()
  }
  fn get_status(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>> {
    self.0.get_status()
  }
  fn wait_for_change(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>> {
    self.0.wait_for_change()
  }
}
forward_debug!(ForwardPower);

struct ForwardWifi(Arc<MockWifi>);
impl WiFiManager for ForwardWifi {
  fn get_status(&self) -> Pin<Box<dyn Future<Output = WifiStatus> + Send + '_>> {
    self.0.get_status()
  }
  fn wait_for_status_change(&self) -> Pin<Box<dyn Future<Output = WifiStatus> + Send + '_>> {
    self.0.wait_for_status_change()
  }
  fn set_desired_state(&self, s: WifiDesiredState) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    self.0.set_desired_state(s)
  }
  fn scan(&self) -> Pin<Box<dyn Future<Output = Result<Vec<WifiResult>, ()>> + Send + '_>> {
    self.0.scan()
  }
}
forward_debug!(ForwardWifi);

struct ForwardInput(Arc<MockInput>);
impl InputManager for ForwardInput {
  fn next_button(&self) -> Pin<Box<dyn Future<Output = HexButton> + Send + '_>> {
    self.0.next_button()
  }
  fn inject_button(&self, b: HexButton) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    self.0.inject_button(b)
  }
}
forward_debug!(ForwardInput);

struct ForwardSystem(Arc<MockSystem>);
impl SystemManager for ForwardSystem {
  fn next_button(&self) -> Pin<Box<dyn Future<Output = SystemMessage> + Send + '_>> {
    self.0.next_button()
  }
  fn inject(&self, m: SystemMessage) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    self.0.inject(m)
  }
}
forward_debug!(ForwardSystem);

struct ForwardHexpansion(Arc<MockHexpansion>);
impl HexpansionManager for ForwardHexpansion {
  fn next_event(&self) -> Pin<Box<dyn Future<Output = HexpansionEvent> + Send + '_>> {
    self.0.next_event()
  }
  fn try_next_event(&self) -> Option<HexpansionEvent> {
    self.0.try_next_event()
  }
  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)> {
    self.0.current_state()
  }
  fn next_device_event(&self) -> Pin<Box<dyn Future<Output = DeviceEvent> + Send + '_>> {
    self.0.next_device_event()
  }
  fn try_next_device_event(&self) -> Option<DeviceEvent> {
    self.0.try_next_device_event()
  }
}
forward_debug!(ForwardHexpansion);

struct ForwardHttp(Arc<MockHttp>);
impl HttpClient for ForwardHttp {
  fn request<'a>(&'a self, req: HttpRequest, channel: &'a crate::platform::HttpEventChannel) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    self.0.request(req, channel)
  }
}
forward_debug!(ForwardHttp);

struct ForwardTcp(Arc<MockTcp>);
impl TcpClient for ForwardTcp {
  fn connect(&self, host: String, port: u16) -> Pin<Box<dyn Future<Output = Result<TcpSession, ()>> + 'static>> {
    self.0.connect(host, port)
  }
}
forward_debug!(ForwardTcp);

struct ForwardStorage(Arc<MockStorage>);
impl LocalFsTrait for ForwardStorage {
  fn format(&self) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    self.0.format()
  }
  fn list_files(&self) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    self.0.list_files()
  }
  fn list_dir(&self, p: String) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    self.0.list_dir(p)
  }
  fn get_file_size(&self, n: String) -> Pin<Box<dyn Future<Output = Result<u32, FsError>> + Send + '_>> {
    self.0.get_file_size(n)
  }
  fn read_binary_chunk(&self, n: String, pos: u32, size: u32) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, FsError>> + Send + '_>> {
    self.0.read_binary_chunk(n, pos, size)
  }
  fn write_binary_chunk(
    &self,
    n: String,
    pos: u32,
    buf: Vec<u8>,
    truncate: bool,
  ) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    self.0.write_binary_chunk(n, pos, buf, truncate)
  }
  fn read_text_file(&self, n: String) -> Pin<Box<dyn Future<Output = Result<String, FsError>> + Send + '_>> {
    self.0.read_text_file(n)
  }
  fn write_text_file(&self, n: String, t: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    self.0.write_text_file(n, t)
  }
  fn delete(&self, n: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    self.0.delete(n)
  }
  fn mkdir(&self, n: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    self.0.mkdir(n)
  }
  fn file_exists(&self, n: String) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
    self.0.file_exists(n)
  }
  fn get_file_type(&self, n: String) -> Pin<Box<dyn Future<Output = Result<FileType, FsError>> + Send + '_>> {
    self.0.get_file_type(n)
  }
}
forward_debug!(ForwardStorage);

struct ForwardConfig(Arc<MockConfig>);
impl ConfigFileTrait<DeviceConfig> for ForwardConfig {
  fn get_json(&self) -> Pin<Box<dyn Future<Output = Result<String, StateError>> + Send + '_>> {
    self.0.get_json()
  }
  fn set_json(&self, j: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>> {
    self.0.set_json(j)
  }
  fn get_data(&self) -> Pin<Box<dyn Future<Output = DeviceConfig> + Send + '_>> {
    self.0.get_data()
  }
  fn set_data(&self, s: DeviceConfig) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    self.0.set_data(s)
  }
  fn save(&self) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>> {
    self.0.save()
  }
}
forward_debug!(ForwardConfig);

struct ForwardSpawner(Arc<MockSpawner>);
impl AppSpawner for ForwardSpawner {
  fn spawn(&self, f: Box<dyn Future<Output = ()> + Send + 'static>) {
    self.0.spawn(f)
  }
  fn spawn_local(&self, f: Box<dyn Future<Output = ()> + 'static>) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    self.0.spawn_local(f)
  }
}
forward_debug!(ForwardSpawner);
