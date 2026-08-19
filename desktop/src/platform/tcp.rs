use app::platform::{TcpClient, TcpEvent, TcpEventChannel, TcpSession, TcpSessionBackend};
use core::{fmt, future::Future, pin::Pin};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Desktop raw TCP client: one `std::net::TcpStream` per session, with a
/// background reader thread streaming inbound bytes into the session's event
/// channel (mirrors the firmware's Embassy-task pump). All per-connection
/// state is owned by the session handle / reader thread and freed when the
/// connection ends.
pub struct DesktopTcpClient;

impl DesktopTcpClient {
  pub fn new() -> Self {
    Self
  }
}

impl Default for DesktopTcpClient {
  fn default() -> Self {
    Self::new()
  }
}

impl fmt::Debug for DesktopTcpClient {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("DesktopTcpClient").finish()
  }
}

struct DesktopTcpSession {
  events: TcpEventChannel,
  /// Write side of the connection, shared with the reader thread's clone.
  writer: Arc<Mutex<Option<TcpStream>>>,
}

impl fmt::Debug for DesktopTcpSession {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("DesktopTcpSession").finish()
  }
}

impl Drop for DesktopTcpSession {
  fn drop(&mut self) {
    // Close the socket when the last handle goes away so the reader thread
    // (which holds a clone) exits instead of running until the peer closes.
    if let Some(stream) = self.writer.lock().unwrap().take() {
      let _ = stream.shutdown(Shutdown::Both);
    }
  }
}

impl TcpSessionBackend for DesktopTcpSession {
  fn next_event<'a>(&'a self) -> Pin<Box<dyn Future<Output = TcpEvent> + 'a>> {
    let events = self.events.clone();
    Box::pin(async move { events.next().await })
  }

  fn try_next_event(&self) -> Option<TcpEvent> {
    self.events.try_next()
  }

  fn send<'a>(&'a self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    let writer = self.writer.clone();
    Box::pin(async move {
      let mut guard = writer.lock().unwrap();
      if let Some(stream) = guard.as_mut() {
        let _ = stream.write_all(&data);
      }
    })
  }

  fn close<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    let writer = self.writer.clone();
    Box::pin(async move {
      if let Some(stream) = writer.lock().unwrap().take() {
        let _ = stream.shutdown(Shutdown::Both);
      }
    })
  }
}

impl TcpClient for DesktopTcpClient {
  fn connect(&self, host: String, port: u16) -> Pin<Box<dyn Future<Output = Result<TcpSession, ()>> + 'static>> {
    Box::pin(async move {
      let stream = match TcpStream::connect((host.as_str(), port)) {
        Ok(s) => s,
        Err(_) => return Err(()),
      };
      let _ = stream.set_nodelay(true);
      let mut reader = match stream.try_clone() {
        Ok(r) => r,
        Err(_) => return Err(()),
      };
      let writer = Arc::new(Mutex::new(Some(stream)));
      let events = TcpEventChannel::new();
      let pump_events = events.clone();

      // Background read pump. `try_push` keeps it non-blocking; when the
      // consumer falls behind we back off briefly and drop the chunk (lossy
      // backpressure) rather than spin at 100% CPU. The thread exits when the
      // read side closes (session dropped/closed, or peer EOF) and its clone
      // of the stream is freed with it.
      std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
          match reader.read(&mut buf) {
            Ok(0) => {
              let _ = pump_events.try_push(TcpEvent::Closed);
              break;
            }
            Ok(n) => {
              if !pump_events.try_push(TcpEvent::Data(buf[..n].to_vec())) {
                std::thread::sleep(Duration::from_millis(5));
              }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => continue,
            Err(_) => {
              let _ = pump_events.try_push(TcpEvent::Error);
              break;
            }
          }
        }
      });

      Ok(TcpSession::new(Arc::new(DesktopTcpSession { events, writer })))
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use app::utils::select_timeout;
  use futures::executor::block_on;

  /// A local echo server that handles `conns` sequential connections: reads
  /// one chunk, echoes it back, and keeps the connection open until the
  /// client closes it.
  fn echo_server(conns: usize) -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
      for _ in 0..conns {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 128];
        if let Ok(n) = stream.read(&mut buf)
          && n > 0
        {
          let _ = stream.write_all(&buf[..n]);
        }
      }
    });
    (addr, handle)
  }

  #[test]
  fn session_lifecycle_connect_send_close_reconnect() {
    let (addr, server) = echo_server(2);
    let client = DesktopTcpClient::new();
    block_on(async {
      // First session: connect, echo round-trip, close, see Closed.
      let session = client.connect(addr.ip().to_string(), addr.port()).await.expect("connect");
      session.send(b"hello".to_vec()).await;
      let event = select_timeout(session.next_event(), 5000).await.expect("echo in time");
      assert!(matches!(event, TcpEvent::Data(d) if d == b"hello"));
      session.close().await;
      let event = select_timeout(session.next_event(), 5000).await.expect("Closed in time");
      assert!(matches!(event, TcpEvent::Closed));
      drop(session);

      // Reconnect: a fresh session works after the first is fully gone.
      let session = client.connect(addr.ip().to_string(), addr.port()).await.expect("reconnect");
      session.send(b"again".to_vec()).await;
      let event = select_timeout(session.next_event(), 5000).await.expect("echo in time");
      assert!(matches!(event, TcpEvent::Data(d) if d == b"again"));
      session.close().await;
    });
    server.join().unwrap();
  }

  #[test]
  fn connect_to_closed_port_fails() {
    // Bind then drop a listener to get a port with nothing on it.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let client = DesktopTcpClient::new();
    assert!(block_on(client.connect(addr.ip().to_string(), addr.port())).is_err());
  }
}
