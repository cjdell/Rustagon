use crate::utils::dns::DnsResolver;
use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use app::platform::{TcpClient, TcpEvent, TcpEventChannel};
use core::{fmt, future::Future, net::SocketAddr, pin::Pin};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_net::{
  tcp::client::{TcpClient as NetTcpClient, TcpClientState, TcpConnection},
  Stack,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, mutex::Mutex};
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
enum TcpCommand {
  Send(Vec<u8>),
  Close,
}

/// Channel the app writes commands to and the pump task drains.
type CmdChannel = Channel<CriticalSectionRawMutex, TcpCommand, 8>;

/// Slot holding the current connection's command channel. The `u64` is a
/// generation counter incremented on every `connect`, so a stale pump task
/// (from a previous connection that was never cleanly closed) can detect that
/// the slot now points at a *different* channel and exit without clearing it.
type CmdSlot = Arc<Mutex<CriticalSectionRawMutex, (u64, Option<&'static CmdChannel>)>>;

/// Embassy task that owns one TCP connection: reads inbound bytes into the
/// event channel and executes write/close commands from the app.
#[embassy_executor::task]
async fn tcp_pump_task(events: &'static TcpEventChannel, cmd: &'static CmdChannel, mut conn: Conn, slot: CmdSlot, generation: u64) {
  let mut buf = [0u8; RX];
  info!("tcp_pump: start gen {generation}");
  let mut closed = false;
  while !closed {
    // Drain queued commands first so writes aren't starved by inbound data.
    while let Ok(command) = cmd.try_receive() {
      match command {
        TcpCommand::Send(data) => {
          info!("tcp_pump: write {} bytes", data.len());
          if let Err(e) = conn.write_all(&data).await {
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

    // Wait for inbound data or another command.
    match select(conn.read(&mut buf), cmd.receive()).await {
      Either::First(Ok(0)) => {
        info!("tcp_pump: eof, sending Closed");
        events.send(TcpEvent::Closed).await;
        break;
      }
      Either::First(Ok(n)) => {
        info!("tcp_pump: read {n} bytes, sending Data");
        events.send(TcpEvent::Data(buf[..n].to_vec())).await;
      }
      Either::First(Err(e)) => {
        warn!("tcp_pump: read error {e:?}");
        events.send(TcpEvent::Error).await;
        break;
      }
      Either::Second(TcpCommand::Send(data)) => {
        info!("tcp_pump: write {} bytes", data.len());
        if let Err(e) = conn.write_all(&data).await {
          warn!("tcp_pump: write error {e:?}");
        }
      }
      Either::Second(TcpCommand::Close) => {
        info!("tcp_pump: close command");
        break;
      }
    }
  }
  // Dropping `conn` closes the socket (its `Drop` impl).
  let mut guard = slot.lock().await;
  if guard.0 == generation {
    guard.1 = None;
  }
  info!("tcp_pump: task done");
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
  fn connect(&self, host: String, port: u16, channel: &'static TcpEventChannel) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
    let stack = self.stack;
    let slot = self.slot.clone();
    Box::pin(async move {
      // Resolve the hostname (or parse a literal IP).
      let resolver = DnsResolver::new(stack);
      let ip = match resolver.get_host_by_name(&host, AddrType::IPv4).await {
        Ok(ip) => ip,
        Err(_) => {
          warn!("tcp: dns failed for {host}");
          channel.send(TcpEvent::Error).await;
          return;
        }
      };
      info!("tcp: resolved {host} -> {ip}");

      // The connection outlives this future (the pump task owns it), so the
      // pool state and client are leaked for the session lifetime.
      let state = Box::leak(Box::new_in(TcpClientState::<1, TX, RX>::new(), ExternalMemory));
      let tcp = Box::leak(Box::new_in(NetTcpClient::new(stack, state), ExternalMemory));

      let connection = match tcp.connect(SocketAddr::new(ip, port)).await {
        Ok(c) => c,
        Err(e) => {
          warn!("tcp connect to {host}:{port} failed: {e:?}");
          channel.send(TcpEvent::Error).await;
          return;
        }
      };
      info!("tcp: connected to {host}:{port}");

      let cmd: &'static CmdChannel = Box::leak(Box::new(CmdChannel::new()));
      let generation = {
        let mut guard = slot.lock().await;
        guard.0 = guard.0.wrapping_add(1);
        guard.1 = Some(cmd);
        guard.0
      };
      info!("tcp: cmd slot set (gen {generation})");

      // The pump task operates on the `!Send` embassy-net connection, so it
      // must be spawned on the current (non-Send) executor. `connect` is only
      // ever called from an embassy task (the menu task), where
      // `for_current_executor` is sound.
      let spawner = unsafe { Spawner::for_current_executor() }.await;
      if spawner
        .spawn(tcp_pump_task(channel, cmd, connection, slot.clone(), generation))
        .is_err()
      {
        warn!("tcp: failed to spawn pump task");
        channel.send(TcpEvent::Error).await;
        return;
      }
      info!("tcp: pump spawned");
      channel.send(TcpEvent::Connected).await;
      info!("tcp: Connected event sent");
    })
  }

  fn send(&self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
    let slot = self.slot.clone();
    Box::pin(async move {
      info!("tcp: send {} bytes", data.len());
      let cmd = {
        let guard = slot.lock().await;
        guard.1
      };
      match cmd {
        Some(c) => {
          c.send(TcpCommand::Send(data)).await;
        }
        None => warn!("tcp: send with no active connection"),
      }
    })
  }

  fn close(&self) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
    let slot = self.slot.clone();
    Box::pin(async move {
      let cmd = {
        let guard = slot.lock().await;
        guard.1
      };
      if let Some(c) = cmd {
        c.send(TcpCommand::Close).await;
      }
    })
  }
}
