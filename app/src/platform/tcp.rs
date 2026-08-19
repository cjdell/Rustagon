use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::{fmt, future::Future, pin::Pin};

use crate::utils::EventQueue;

/// Events produced by an open TCP connection. The connection is pumped by the
/// platform in the background (an Embassy task on firmware, a thread on
/// desktop); the consumer reads these events off the session handle.
///
/// The connect/fail outcome is *not* an event — [`TcpClient::connect`]
/// resolves to `Ok(TcpSession)` once the connection is established and
/// `Err(())` on failure, so there is no `Connected` variant to wait for.
#[derive(Debug, Clone)]
pub enum TcpEvent {
  /// A chunk of inbound bytes.
  Data(Vec<u8>),
  /// The peer closed the connection (or it was closed locally).
  Closed,
  /// The connection failed or was reset.
  Error,
}

/// Queue carrying [`TcpEvent`]s from the connection pump to the session
/// handle. Platform-internal: created per connection, cloned into the
/// [`TcpSession`], and freed when the last handle and the pump are gone.
/// Backed by [`EventQueue`], so every occurrence is delivered (no polling).
pub type TcpEventChannel = EventQueue<TcpEvent, 16>;

/// Platform backend for [`TcpSession`].
///
/// The futures returned here are deliberately **not** required to be `Send`:
/// on the firmware the connection wraps an `embassy-net` socket (which is
/// `!Send`), so they only ever run on the single-core async executor. The
/// backend itself stays `Send + Sync` so a [`TcpSession`] handle (an
/// `Arc<dyn TcpSessionBackend>`) can live inside `Send` app state.
pub trait TcpSessionBackend: Send + Sync + fmt::Debug {
  /// Wait for the next inbound event.
  fn next_event<'a>(&'a self) -> Pin<Box<dyn Future<Output = TcpEvent> + 'a>>;
  /// Consume the next inbound event without blocking, if one is queued.
  fn try_next_event(&self) -> Option<TcpEvent>;
  /// Queue `data` for writing to the connection. Write errors are surfaced
  /// later as [`TcpEvent::Error`] (the pump owns the socket exclusively).
  fn send<'a>(&'a self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = ()> + 'a>>;
  /// Ask the connection pump to close. The pump sends [`TcpEvent::Closed`]
  /// before it exits and frees everything it owns.
  fn close<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + 'a>>;
}

/// A cloneable handle to an open TCP connection.
///
/// All per-connection state (socket, event channel, pump) is owned by the
/// platform and freed when the connection ends — no `Box::leak`. Dropping the
/// last handle also closes the connection on both platforms.
#[derive(Clone, Debug)]
pub struct TcpSession {
  inner: Arc<dyn TcpSessionBackend>,
}

impl TcpSession {
  pub fn new(backend: Arc<dyn TcpSessionBackend>) -> Self {
    Self { inner: backend }
  }

  /// Wait for the next inbound event.
  pub async fn next_event(&self) -> TcpEvent {
    self.inner.next_event().await
  }

  /// Consume the next inbound event without blocking, if one is queued.
  pub fn try_next_event(&self) -> Option<TcpEvent> {
    self.inner.try_next_event()
  }

  /// Queue `data` for writing to the connection.
  pub async fn send(&self, data: Vec<u8>) {
    self.inner.send(data).await
  }

  /// Ask the connection pump to close.
  pub async fn close(&self) {
    self.inner.close().await
  }
}

/// Raw TCP stream abstraction.
///
/// `connect` opens a connection to `host:port` and returns a
/// [`TcpSession`] once it is established (`Err(())` on DNS/connection
/// failure). Inbound bytes are streamed to the session by a platform-owned
/// background pump until the connection closes.
///
/// The returned future is deliberately **not** required to be `Send`: on the
/// firmware the connection wraps an `embassy-net` socket (which is `!Send`
/// due to internal `UnsafeCell`s), so the future only ever runs on the
/// single-core async executor. The `TcpClient` trait itself remains
/// `Send + Sync` so it can live behind `Arc<dyn TcpClient>` in a `Send + Sync`
/// `Platform`.
pub trait TcpClient: Send + Sync + fmt::Debug {
  /// Open a TCP connection. Resolves to a session handle on success.
  fn connect(&self, host: String, port: u16) -> Pin<Box<dyn Future<Output = Result<TcpSession, ()>> + 'static>>;
}

#[derive(Clone, Debug)]
pub struct TcpHandle {
  inner: Arc<dyn TcpClient>,
}

impl TcpHandle {
  pub fn new(client: Arc<dyn TcpClient>) -> Self {
    Self { inner: client }
  }

  pub async fn connect(&self, host: String, port: u16) -> Result<TcpSession, ()> {
    self.inner.connect(host, port).await
  }
}
