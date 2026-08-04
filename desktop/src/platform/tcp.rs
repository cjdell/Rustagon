use app::platform::{TcpClient, TcpEvent, TcpEventChannel};
use core::{fmt, future::Future, pin::Pin};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Desktop raw TCP client: `std::net::TcpStream` with a background reader
/// thread streaming inbound bytes into the event channel (mirrors the
/// firmware's Embassy-task pump).
pub struct DesktopTcpClient {
  /// The active connection's write side, shared with `send`/`close`.
  writer: Arc<Mutex<Option<TcpStream>>>,
}

impl DesktopTcpClient {
  pub fn new() -> Self {
    Self {
      writer: Arc::new(Mutex::new(None)),
    }
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

impl TcpClient for DesktopTcpClient {
  fn connect(&self, host: String, port: u16, channel: &'static TcpEventChannel) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
    let writer = self.writer.clone();
    Box::pin(async move {
      let stream = match TcpStream::connect((host.as_str(), port)) {
        Ok(s) => s,
        Err(_) => {
          channel.send(TcpEvent::Error).await;
          return;
        }
      };
      let _ = stream.set_nodelay(true);
      let mut reader = match stream.try_clone() {
        Ok(r) => r,
        Err(_) => {
          channel.send(TcpEvent::Error).await;
          return;
        }
      };
      *writer.lock().unwrap() = Some(stream);

      // Background read pump. `try_send` keeps it non-blocking; when the
      // consumer falls behind we back off briefly and drop the chunk (lossy
      // backpressure) rather than spin at 100% CPU.
      std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
          match reader.read(&mut buf) {
            Ok(0) => {
              let _ = channel.try_send(TcpEvent::Closed);
              break;
            }
            Ok(n) => {
              let _ = match channel.try_send(TcpEvent::Data(buf[..n].to_vec())) {
                Ok(_) => Ok(()),
                Err(_) => {
                  std::thread::sleep(Duration::from_millis(5));
                  Err(())
                }
              };
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => continue,
            Err(_) => {
              let _ = channel.try_send(TcpEvent::Error);
              break;
            }
          }
        }
      });

      channel.send(TcpEvent::Connected).await;
    })
  }

  fn send(&self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
    let writer = self.writer.clone();
    Box::pin(async move {
      let mut guard = writer.lock().unwrap();
      if let Some(stream) = guard.as_mut() {
        let _ = stream.write_all(&data);
      }
    })
  }

  fn close(&self) -> Pin<Box<dyn Future<Output = ()> + 'static>> {
    let writer = self.writer.clone();
    Box::pin(async move {
      let mut guard = writer.lock().unwrap();
      if let Some(stream) = guard.take() {
        let _ = stream.shutdown(Shutdown::Both);
      }
    })
  }
}
