use crate::utils::dns::DnsResolver;
use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use app::platform::{TcpClient, TcpEvent, TcpEventChannel, TcpSession, TcpSessionBackend};
use app::utils::EventQueue;
use core::{
  fmt,
  future::Future,
  net::SocketAddr,
  pin::Pin,
  sync::atomic::{AtomicBool, Ordering},
};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_net::{
  tcp::client::{TcpClient as NetTcpClient, TcpClientState, TcpConnection},
  Stack,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embedded_io_async::{Read, Write};
use embedded_nal_async::{AddrType, Dns, TcpConnect};
use esp_alloc::ExternalMemory;
use log::{info, warn};

/// TX/RX buffer sizes for each pooled SSH connection.
const TX: usize = 2048;
const RX: usize = 2048;

/// A single pooled TCP connection (the pool has room for one).
type Conn = TcpConnection<'static, 1, TX, RX>;

/// Commands the app sends to the connection's pump task. The pump owns the
/// connection exclusively, so `send`/`close` never touch it directly — they
/// queue a command instead. This avoids sharing the `!Send` socket across
/// tasks (which would otherwise need a mutex, and a mutex held across a
/// blocking read would starve `send` on the single-core executor).
#[derive(Clone)]
enum TcpCommand {
  Send(Vec<u8>),
  Close,
}

/// Queue the session sends commands to and the pump task drains.
type CmdChannel = EventQueue<TcpCommand, 8>;

/// Slot holding the current connection's command channel. The `u64` is a
/// generation counter incremented on every `connect`, so a stale pump task
/// (from a previous connection that was never cleanly closed) can detect that
/// the slot now points at a *different* channel and exit without clearing it.
/// The queue is owned (`EventQueue` is cloneable — internally an `Arc`),
/// never leaked.
type CmdSlot = Arc<Mutex<CriticalSectionRawMutex, (u64, Option<CmdChannel>)>>;

/// Set to `false` when either the last session handle is dropped or the pump
/// exits. Lets command/event sends give up instead of blocking forever on a
/// channel nobody is draining/reading.
type Alive = Arc<AtomicBool>;

/// Owned PSRAM allocation of the embassy-net pool state and the client that
/// borrows it. The `state` box is never moved (a `Box` is a pointer, so the
/// allocation itself stays put no matter where this struct lives), and
/// `client` is declared first so it is dropped *before* `state` when this
/// struct is freed.
struct OwnedNetTcp {
  client: NetTcpClient<'static, 1, TX, RX>,
  // Never read: the field exists to own the allocation that `client`
  // borrows. Dropped after `client` (field order), which is the only free.
  #[allow(dead_code)]
  state: Box<TcpClientState<1, TX, RX>, ExternalMemory>,
}

impl OwnedNetTcp {
  fn new(stack: Stack<'static>) -> Self {
    // SAFETY: `client` holds a `'static` reference into the heap allocation
    // owned by `state`. That allocation is never moved, never written after
    // this point, and is freed only when this struct is dropped — after
    // `client` (field drop order).
    let state = Box::new_in(TcpClientState::<1, TX, RX>::new(), ExternalMemory);
    let state_ptr: *const TcpClientState<1, TX, RX> = &*state;
    let client = unsafe { NetTcpClient::new(stack, &*state_ptr) };
    Self { client, state }
  }
}

/// Everything the pump task owns for one connection. `conn` borrows
/// `net.client`, which borrows `net.state`, so the `net` allocation must be
/// freed *after* `conn` is dropped: field order drops `conn` first, and the
/// explicit `Drop` below frees the allocation only then.
struct Pump {
  conn: Conn,
  net: *mut OwnedNetTcp,
}

impl Drop for Pump {
  fn drop(&mut self) {
    // `conn` was already dropped (fields drop in declaration order): the
    // socket is closed and its pool buffers returned to the state before the
    // allocation is freed.
    unsafe {
      drop(Box::from_raw_in(self.net, ExternalMemory));
    }
  }
}

/// Queue `command` on the pump's command channel, tolerating a full queue.
/// Gives up if the pump has exited (or the session was dropped) — a dead
/// pump never drains the channel, so blocking here would hang forever.
async fn send_cmd(cmd: &CmdChannel, alive: &AtomicBool, command: TcpCommand) {
  while !cmd.try_push(command.clone()) {
    if !alive.load(Ordering::Acquire) {
      return;
    }
    embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
  }
}

/// Queue `ev` on the event queue, tolerating a slow consumer. Returns
/// `false` if the session handle has been dropped (consumer gone) — the
/// pump should then exit and free the connection instead of spinning.
async fn send_event(events: &TcpEventChannel, alive: &AtomicBool, ev: TcpEvent) -> bool {
  while !events.try_push(ev.clone()) {
    if !alive.load(Ordering::Acquire) {
      return false;
    }
    embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
  }
  true
}

/// Resolves once the session handle has been dropped (`alive` cleared).
/// Checked every 200 ms so a pump idle in `select` can never outlive its
/// consumer (e.g. a session dropped without an explicit `close`).
async fn session_gone(alive: &AtomicBool) {
  loop {
    if !alive.load(Ordering::Acquire) {
      return;
    }
    embassy_time::Timer::after(embassy_time::Duration::from_millis(200)).await;
  }
}

/// Embassy task that owns one TCP connection end to end: reads inbound bytes
/// into the event channel and executes write/close commands from the session.
/// Everything it is given (connection, pool state/client, both channels) is
/// *owned* and freed when the task ends — nothing is leaked per connection.
#[embassy_executor::task]
async fn tcp_pump_task(events: TcpEventChannel, cmd: CmdChannel, mut pump: Pump, slot: CmdSlot, generation: u64, alive: Alive) {
  let mut buf = [0u8; RX];
  info!("tcp_pump: start gen {generation}");
  let mut closed = false;
  while !closed {
    // Drain queued commands first so writes aren't starved by inbound data.
    while let Some(command) = cmd.try_next() {
      match command {
        TcpCommand::Send(data) => {
          info!("tcp_pump: write {} bytes", data.len());
          if let Err(e) = pump.conn.write_all(&data).await {
            warn!("tcp_pump: write error {e:?}");
          }
        }
        TcpCommand::Close => {
          info!("tcp_pump: close command");
          closed = true;
          break;
        }
      }
    }
    if closed {
      break;
    }

    // Wait for inbound data, a command, or the session being dropped
    // (whichever first) so the pump can never outlive its consumer.
    match select(pump.conn.read(&mut buf), async {
      match select(cmd.next(), session_gone(&alive)).await {
        Either::First(command) => Some(command),
        Either::Second(()) => None,
      }
    })
    .await
    {
      Either::First(Ok(0)) => {
        info!("tcp_pump: eof, sending Closed");
        let _ = send_event(&events, &alive, TcpEvent::Closed).await;
        break;
      }
      Either::First(Ok(n)) => {
        info!("tcp_pump: read {n} bytes, sending Data");
        if !send_event(&events, &alive, TcpEvent::Data(buf[..n].to_vec())).await {
          warn!("tcp_pump: session gone, aborting pump");
          break;
        }
      }
      Either::First(Err(e)) => {
        warn!("tcp_pump: read error {e:?}");
        let _ = send_event(&events, &alive, TcpEvent::Error).await;
        break;
      }
      Either::Second(Some(TcpCommand::Send(data))) => {
        info!("tcp_pump: write {} bytes", data.len());
        if let Err(e) = pump.conn.write_all(&data).await {
          warn!("tcp_pump: write error {e:?}");
        }
      }
      Either::Second(Some(TcpCommand::Close)) => {
        info!("tcp_pump: close command");
        closed = true;
      }
      Either::Second(None) => {
        info!("tcp_pump: session dropped, exiting");
        closed = true;
      }
    }
  }
  // Locally closed: tell the session (best effort — skipped if it's gone).
  if closed {
    let _ = send_event(&events, &alive, TcpEvent::Closed).await;
  }
  // Dropping `pump` frees everything: the socket closes (and its pool buffers
  // are returned) before the pool state/client box is deallocated.
  drop(pump);
  let mut guard = slot.lock().await;
  if guard.0 == generation {
    guard.1 = None;
  }
  alive.store(false, Ordering::Release);
  info!("tcp_pump: task done gen {generation}, all state freed");
}

struct HardwareTcpSession {
  events: TcpEventChannel,
  cmd: CmdChannel,
  alive: Alive,
}

impl fmt::Debug for HardwareTcpSession {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareTcpSession").finish()
  }
}

impl Drop for HardwareTcpSession {
  fn drop(&mut self) {
    // The last handle went away: mark the session dead so the pump can exit
    // (freeing the connection) and best-effort ask it to close now.
    self.alive.store(false, Ordering::Release);
    let _ = self.cmd.try_push(TcpCommand::Close);
  }
}

impl TcpSessionBackend for HardwareTcpSession {
  fn next_event<'a>(&'a self) -> Pin<Box<dyn Future<Output = TcpEvent> + 'a>> {
    let events = self.events.clone();
    Box::pin(async move { events.next().await })
  }

  fn try_next_event(&self) -> Option<TcpEvent> {
    self.events.try_next()
  }

  fn send<'a>(&'a self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    let cmd = self.cmd.clone();
    let alive = self.alive.clone();
    Box::pin(async move { send_cmd(&cmd, &alive, TcpCommand::Send(data)).await })
  }

  fn close<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    let cmd = self.cmd.clone();
    let alive = self.alive.clone();
    Box::pin(async move { send_cmd(&cmd, &alive, TcpCommand::Close).await })
  }
}

/// Hardware TCP client wrapping the ESP32 network stack.
///
/// Safety: `Stack<'static>` is `!Send + !Sync`; this type is only used from the
/// single-core async executor, serialized like `HardwareHttpClient`.
pub struct HardwareTcpClient {
  stack: Stack<'static>,
  /// Command channel to the current connection's pump task.
  slot: CmdSlot,
}

impl HardwareTcpClient {
  pub fn new(stack: Stack<'static>) -> Self {
    Self {
      stack,
      slot: Arc::new(Mutex::new((0, None))),
    }
  }
}

// Safety: only used from a single async executor core
unsafe impl Send for HardwareTcpClient {}
unsafe impl Sync for HardwareTcpClient {}

impl fmt::Debug for HardwareTcpClient {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareTcpClient").finish()
  }
}

impl Clone for HardwareTcpClient {
  fn clone(&self) -> Self {
    Self {
      stack: self.stack,
      slot: self.slot.clone(),
    }
  }
}

impl TcpClient for HardwareTcpClient {
  fn connect(&self, host: String, port: u16) -> Pin<Box<dyn Future<Output = Result<TcpSession, ()>> + 'static>> {
    let stack = self.stack;
    let slot = self.slot.clone();
    Box::pin(async move {
      // Resolve the hostname (or parse a literal IP).
      let resolver = DnsResolver::new(stack);
      let ip = match resolver.get_host_by_name(&host, AddrType::IPv4).await {
        Ok(ip) => ip,
        Err(_) => {
          warn!("tcp: dns failed for {host}");
          return Err(());
        }
      };
      info!("tcp: resolved {host} -> {ip}");

      // The pool state + client live in one owned PSRAM allocation. A
      // `'static` reference into it lets `connect` hand back a
      // `TcpConnection<'static>` that does not borrow any local; the pump
      // task takes ownership of the raw pointer and is the sole freer.
      //
      // SAFETY: the `'static` client reference is valid until `Pump::drop`
      // frees the allocation; the connection (which borrows the client) is
      // dropped first, and no other references to the allocation exist.
      let mut net_box = Box::new_in(OwnedNetTcp::new(stack), ExternalMemory);
      // Take ownership of the allocation, suppressing the box's drop glue so
      // the allocation is freed exactly once — by `Pump::drop` (or the
      // connect-failure path below). (The esp toolchain's `Box` has no
      // `into_raw` for custom allocators.)
      let ptr: *mut OwnedNetTcp = core::ptr::addr_of_mut!(*net_box);
      core::mem::forget(net_box);
      let net = ptr;
      let client: &'static NetTcpClient<'static, 1, TX, RX> = unsafe { &(*net).client };

      let connection = match client.connect(SocketAddr::new(ip, port)).await {
        Ok(c) => c,
        Err(e) => {
          warn!("tcp connect to {host}:{port} failed: {e:?}");
          unsafe {
            drop(Box::from_raw_in(net, ExternalMemory));
          }
          return Err(());
        }
      };
      info!("tcp: connected to {host}:{port}");

      let cmd = CmdChannel::new();
      let events = TcpEventChannel::new();
      let alive = Arc::new(AtomicBool::new(true));

      let generation = {
        let mut guard = slot.lock().await;
        // Replace any stale pump from a previous connection: ask it to close
        // so its socket is freed, then point the slot at the new channel.
        if let Some(old) = guard.1.take() {
          let _ = old.try_push(TcpCommand::Close);
        }
        guard.0 = guard.0.wrapping_add(1);
        guard.1 = Some(cmd.clone());
        guard.0
      };
      info!("tcp: cmd slot set (gen {generation})");

      // The pump task operates on the `!Send` embassy-net connection, so it
      // must be spawned on the current (non-Send) executor. `connect` is only
      // ever called from an embassy task (the menu task), where
      // `for_current_executor` is sound.
      let spawner = unsafe { Spawner::for_current_executor() }.await;
      let pump = Pump { conn: connection, net };
      match tcp_pump_task(events.clone(), cmd.clone(), pump, slot.clone(), generation, alive.clone()) {
        Ok(token) => {
          spawner.spawn(token);
          info!("tcp: pump spawned");
          Ok(TcpSession::new(Arc::new(HardwareTcpSession { events, cmd, alive })))
        }
        Err(_) => {
          warn!("tcp: failed to spawn pump task");
          Err(())
        }
      }
    })
  }
}
