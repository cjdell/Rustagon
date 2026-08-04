use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::{fmt, future::Future, pin::Pin};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};

/// Events produced by an open TCP connection. The connection is pumped by the
/// platform in the background (an Embassy task on firmware, a thread on
/// desktop); the consumer reads these events off the channel.
#[derive(Debug, Clone)]
pub enum TcpEvent {
  /// The connection was established and the pump is now streaming inbound data.
  Connected,
  /// A chunk of inbound bytes.
  Data(Vec<u8>),
  /// The peer closed the connection (or it was closed locally).
  Closed,
  /// The connection failed or was reset.
  Error,
}

/// Bounded channel carrying [`TcpEvent`]s from the connection pump to the
/// consumer. The channel is leaked to `'static` for the lifetime of a session
/// so the background pump can hold it without a borrow lifetime.
pub type TcpEventChannel = Channel<CriticalSectionRawMutex, TcpEvent, 16>;

/// Raw TCP stream abstraction.
///
/// `connect` opens a connection to `host:port` and starts a background pump
/// that pushes inbound bytes into the channel as `TcpEvent::Data` until the
/// connection closes. `send` writes outbound bytes to the same connection.
///
/// The returned futures are deliberately **not** required to be `Send`: on the
/// firmware the connection wraps an `embassy-net` socket (which is `!Send` due
/// to internal `UnsafeCell`s), so the futures only ever run on the single-core
/// async executor. The `TcpClient` trait itself remains `Send + Sync` so it can
/// live behind `Arc<dyn TcpClient>` in a `Send + Sync` `Platform`.
pub trait TcpClient: Send + Sync + fmt::Debug {
  /// Open a TCP connection and begin streaming inbound bytes into `channel`.
  /// The returned future completes once the connection is established (or
  /// fails); the channel carries `Connected`/`Error` to signal the outcome.
  /// The pump keeps running until the connection closes, independent of the
  /// caller polling this future.
  fn connect(&self, host: String, port: u16, channel: &'static TcpEventChannel) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
  /// Write `data` to the current connection.
  fn send(&self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
  /// Close the current connection.
  fn close(&self) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
}

#[derive(Clone, Debug)]
pub struct TcpHandle {
  inner: Arc<dyn TcpClient>,
}

impl TcpHandle {
  pub fn new(client: Arc<dyn TcpClient>) -> Self {
    Self { inner: client }
  }

  pub async fn connect(&self, host: String, port: u16, channel: &'static TcpEventChannel) {
    self.inner.connect(host, port, channel).await
  }

  pub async fn send(&self, data: Vec<u8>) {
    self.inner.send(data).await
  }

  pub async fn close(&self) {
    self.inner.close().await
  }
}
