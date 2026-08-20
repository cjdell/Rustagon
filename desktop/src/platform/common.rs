use app::platform::hexpansion::HexpansionManager;
use app::platform::led::{LedError, LedManager};
use app::platform::power::{PowerManager, PowerStatus};
use app::platform::system::SystemManager;
use app::platform::wifi::{WiFiManager, WifiStatus};
use app::platform::AppSpawner;
use app::types::{DeviceEvent, HexpansionEvent, HexpansionInfo, LedRequest, SystemMessage, WifiDesiredState, WifiResult};
use app::utils::{EventQueue, WatchedValue};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// Poll cadence for the desktop hexpansion file simulation.
const HEXPANSION_SIM_POLL_MS: u64 = 1000;
/// Hexpansion ports (1-based, matching the firmware's six hexpansion slots).
const HEXPANSION_PORTS: u8 = 6;

/// On-JSON shape of a simulated hexpansion slot file
/// (`<data_dir>/hexpansions/port<N>.json`). The `driver` tag is informational
/// (the desktop has no driver table); see `desktop/data/hexpansions/` for
/// samples.
#[derive(serde::Deserialize)]
struct SimHexpansion {
  vid: u16,
  pid: u16,
  unique_id: u32,
  #[serde(default)]
  friendly_name: String,
  #[serde(default)]
  #[allow(dead_code)]
  driver: Option<String>,
}

/// Read the simulated slot state from `<dir>/port<N>.json` (ports 1..=6).
/// A missing or unreadable file means the port is empty.
fn read_sim_slots(dir: &Path) -> Vec<Option<HexpansionInfo>> {
  (1..=HEXPANSION_PORTS)
    .map(|port| {
      let path = dir.join(format!("port{port}.json"));
      let data = std::fs::read(&path).ok()?;
      let sim: SimHexpansion = serde_json::from_slice(&data).ok()?;
      Some(HexpansionInfo {
        port,
        vid: sim.vid,
        pid: sim.pid,
        unique_id: sim.unique_id,
        friendly_name: sim.friendly_name,
      })
    })
    .collect()
}

/// Background thread for the hexpansion file simulation: re-reads the slot
/// files on a fixed cadence and emits `Inserted`/`Removed` for the slots that
/// changed (same event semantics as the firmware's EEPROM polling task).
fn hexpansion_sim_loop(dir: PathBuf, state: Arc<Mutex<Vec<Option<HexpansionInfo>>>>, events: HexpansionEventQueue) {
  loop {
    let current = read_sim_slots(&dir);
    let mut guard = state.lock().unwrap();
    if *guard != current {
      for (i, (prev, now)) in guard.iter().zip(&current).enumerate() {
        let port = (i + 1) as u8;
        match (prev, now) {
          (None, Some(info)) => {
            let _ = events.try_push(HexpansionEvent::Inserted(info.clone()));
          }
          (Some(_), None) => {
            let _ = events.try_push(HexpansionEvent::Removed { port });
          }
          (Some(prev_info), Some(now_info)) => {
            // In place: re-emit Inserted if the device identity changed
            // (e.g. the user swapped the file's vid/pid/unique_id).
            if (prev_info.vid, prev_info.pid, prev_info.unique_id) != (now_info.vid, now_info.pid, now_info.unique_id) {
              let _ = events.try_push(HexpansionEvent::Inserted(now_info.clone()));
            }
          }
          (None, None) => {}
        }
      }
      *guard = current;
    }
    drop(guard);
    std::thread::sleep(std::time::Duration::from_millis(HEXPANSION_SIM_POLL_MS));
  }
}

/// Fake WiFi environment for the desktop: the AP list comes from
/// `<data_dir>/wifi.json` and the status flips with `set_desired_state`, so
/// `wait_for_status_change` actually resolves (see `desktop/data/wifi.json`
/// for the format).
#[derive(serde::Deserialize)]
struct SimWifiAp {
  ssid: String,
  signal_strength: i8,
  #[serde(default)]
  password_required: bool,
}

/// IP a "connected" desktop WiFi reports (loopback — there is no network).
const DESKTOP_WIFI_IP: core::net::Ipv4Addr = core::net::Ipv4Addr::new(127, 0, 0, 1);

fn read_wifi_scan(data_dir: &Path) -> Vec<WifiResult> {
  let path = data_dir.join("wifi.json");
  let data = match std::fs::read(&path) {
    Ok(d) => d,
    Err(_) => return Vec::new(),
  };
  match serde_json::from_slice::<Vec<SimWifiAp>>(&data) {
    Ok(aps) => aps
      .into_iter()
      .map(|ap| WifiResult {
        ssid: ap.ssid,
        signal_strength: ap.signal_strength,
        password_required: ap.password_required,
      })
      .collect(),
    Err(e) => {
      log::warn!("DesktopWifiManager: bad {}: {e}", path.display());
      Vec::new()
    }
  }
}

/// Desktop background-task spawner: each spawned task runs on its own std
/// thread, driven by a `futures::executor::block_on` (the desktop host has no
/// shared async runtime).
#[derive(Clone, Debug)]
pub struct DesktopAppSpawner;

/// Wrapper letting a `!Send` future box move to a worker thread. Sound on
/// desktop: every future an app builds there is `Send` (std-backed platform);
/// the `!Send` case is a firmware-only concern (embassy-net sockets).
struct SendFut(Box<dyn std::future::Future<Output = ()> + 'static>);

unsafe impl Send for SendFut {}

impl SendFut {
  fn run(self) {
    let SendFut(fut) = self;
    futures::executor::block_on(std::pin::Pin::from(fut));
  }
}

impl AppSpawner for DesktopAppSpawner {
  fn spawn(&self, fut: Box<dyn std::future::Future<Output = ()> + Send + 'static>) {
    std::thread::spawn(move || SendFut(fut).run());
  }

  fn spawn_local(
    &self,
    fut: Box<dyn std::future::Future<Output = ()> + 'static>,
  ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + '_>> {
    // Wrap *before* the closure so it captures the `Send` wrapper as a whole,
    // not the `!Send` box field.
    let wrapper = SendFut(fut);
    Box::pin(async move {
      std::thread::spawn(move || wrapper.run());
    })
  }
}

#[derive(Debug)]
pub struct DesktopLedManager;
impl LedManager for DesktopLedManager {
  fn request(&self, _: LedRequest) -> Result<(), LedError> {
    Ok(())
  }
}

#[derive(Debug)]
pub struct DesktopPowerManager;
impl PowerManager for DesktopPowerManager {
  fn power_off(&self) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async {
      std::process::exit(0);
    })
  }
  fn get_status(&self) -> Pin<Box<dyn std::future::Future<Output = PowerStatus> + Send + '_>> {
    Box::pin(async {
      PowerStatus {
        vbat_mv: 3700,
        vsys_mv: 3700,
        vbus_mv: 0,
        charge_current_ma: 0,
        charge_voltage_mv: 4200,
        input_current_limit_ma: 500,
        is_charging: false,
        is_power_present: false,
        battery_fault: false,
      }
    })
  }
  fn wait_for_change(&self) -> Pin<Box<dyn std::future::Future<Output = PowerStatus> + Send + '_>> {
    // The desktop power status is a fixed stub and never changes.
    Box::pin(std::future::pending())
  }
}

/// Same structure as the firmware's `HardwareWifiManager`: a single
/// `WatchedValue<WifiStatus>` as the source of truth. The desktop simulates
/// the radio: `scan()` reads the AP list from `<data_dir>/wifi.json` and
/// `set_desired_state` flips the status so `wait_for_status_change` resolves.
#[derive(Clone, Debug)]
pub struct DesktopWifiManager {
  status: WatchedValue<WifiStatus>,
  data_dir: PathBuf,
}

impl DesktopWifiManager {
  pub fn new(data_dir: impl Into<PathBuf>) -> Self {
    Self {
      status: WatchedValue::new(WifiStatus::Offline),
      data_dir: data_dir.into(),
    }
  }
}

impl WiFiManager for DesktopWifiManager {
  fn get_status(&self) -> Pin<Box<dyn std::future::Future<Output = WifiStatus> + Send + '_>> {
    Box::pin(self.status.get())
  }
  fn wait_for_status_change(&self) -> Pin<Box<dyn std::future::Future<Output = WifiStatus> + Send + '_>> {
    self.status.wait_for_change()
  }
  fn set_desired_state(&self, state: WifiDesiredState) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    let status = self.status.clone();
    Box::pin(async move {
      match state {
        WifiDesiredState::Online => {
          log::info!("DesktopWifiManager: simulated connect");
          status.set(WifiStatus::Connected(DESKTOP_WIFI_IP)).await;
        }
        WifiDesiredState::Offline => {
          log::info!("DesktopWifiManager: simulated disconnect");
          status.set(WifiStatus::Offline).await;
        }
      }
    })
  }
  fn scan(&self) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<WifiResult>, ()>> + Send + '_>> {
    let data_dir = self.data_dir.clone();
    Box::pin(async move { Ok(read_wifi_scan(&data_dir)) })
  }
}

/// Same depth as the firmware's system event queue.
const SYSTEM_EVENT_QUEUE_DEPTH: usize = 8;

type SystemEventQueue = EventQueue<SystemMessage, SYSTEM_EVENT_QUEUE_DEPTH>;

#[derive(Clone, Debug)]
pub struct DesktopSystemManager {
  events: SystemEventQueue,
}

impl DesktopSystemManager {
  pub fn new() -> Self {
    Self {
      events: SystemEventQueue::new(),
    }
  }

  /// Called from the minifb thread when the boot button equivalent is pressed.
  /// Clones of the manager share the queue, so the minifb thread keeps a clone.
  pub fn push_message(&self, msg: SystemMessage) {
    // Drop on overflow, same as the firmware button task's `try_push`.
    let _ = self.events.try_push(msg);
  }
}

impl SystemManager for DesktopSystemManager {
  fn next_button(&self) -> Pin<Box<dyn std::future::Future<Output = SystemMessage> + Send + '_>> {
    Box::pin(self.events.next())
  }
  fn inject(&self, msg: SystemMessage) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(self.events.push(msg))
  }
}

/// Same depth as the firmware's hexpansion event queue.
const HEXPANSION_EVENT_QUEUE_DEPTH: usize = 16;

/// Same depth as the firmware's `DeviceEventQueue`.
const DEVICE_EVENT_QUEUE_DEPTH: usize = 32;

type HexpansionEventQueue = EventQueue<HexpansionEvent, HEXPANSION_EVENT_QUEUE_DEPTH>;
type DeviceEventQueue = EventQueue<DeviceEvent, DEVICE_EVENT_QUEUE_DEPTH>;

/// Mirrors `HardwareHexpansionManager`: the manager owns both event queues
/// rather than module-level statics, plus the file-backed slot simulation
/// state (a background thread polls `<sim_dir>/port<N>.json` and pushes
/// `Inserted`/`Removed`; see `read_sim_slots` for the format).
#[derive(Clone, Debug)]
pub struct DesktopHexpansionManager {
  events: HexpansionEventQueue,
  device_events: DeviceEventQueue,
  slots: Arc<Mutex<Vec<Option<HexpansionInfo>>>>,
}

impl DesktopHexpansionManager {
  /// `sim_dir` holds the per-slot simulation files
  /// (conventionally `<data_dir>/hexpansions`).
  pub fn new(sim_dir: impl Into<PathBuf>) -> Self {
    let sim_dir = sim_dir.into();
    let events = HexpansionEventQueue::new();
    let slots = Arc::new(Mutex::new(vec![None; HEXPANSION_PORTS as usize]));
    // Seed the shared state with whatever is already plugged in: no initial
    // `Inserted` storm — the viewer reads `current_state` for its initial
    // screen, events track *changes* from here on.
    *slots.lock().unwrap() = read_sim_slots(&sim_dir);
    let sim_events = events.clone();
    let sim_slots = slots.clone();
    std::thread::spawn(move || hexpansion_sim_loop(sim_dir, sim_slots, sim_events));
    Self {
      events,
      device_events: DeviceEventQueue::new(),
      slots,
    }
  }

  /// Injection point for hexpansion plug/unplug events (the simulation thread
  /// feeds the queue directly; tests and future remote control use this).
  #[allow(dead_code)]
  pub fn push_event(&self, event: HexpansionEvent) {
    let _ = self.events.try_push(event);
  }

  /// Called from the minifb thread when a keyboard hexpansion key is pressed/released.
  /// Clones of the manager share the queue, so the minifb thread keeps a clone.
  pub fn push_device_event(&self, event: DeviceEvent) {
    // Drop on overflow, same as the firmware driver's `try_push`.
    let _ = self.device_events.try_push(event);
  }
}

impl HexpansionManager for DesktopHexpansionManager {
  fn next_event(&self) -> Pin<Box<dyn std::future::Future<Output = HexpansionEvent> + Send + '_>> {
    Box::pin(self.events.next())
  }

  fn try_next_event(&self) -> Option<HexpansionEvent> {
    self.events.try_next()
  }

  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)> {
    let slots = self.slots.lock().unwrap();
    (1..=HEXPANSION_PORTS)
      .map(|port| (port, slots[(port - 1) as usize].clone()))
      .collect()
  }

  fn next_device_event(&self) -> Pin<Box<dyn std::future::Future<Output = DeviceEvent> + Send + '_>> {
    Box::pin(self.device_events.next())
  }

  fn try_next_device_event(&self) -> Option<DeviceEvent> {
    self.device_events.try_next()
  }
}
