use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;
use core::future::Future;
use core::pin::Pin;

pub use crate::types::SystemMessage;

pub trait SystemManager: Send + Sync + fmt::Debug {
  fn next_button(&self) -> Pin<Box<dyn Future<Output = SystemMessage> + Send + '_>>;

  /// Inject a synthetic system event (e.g. remote control over WebSocket).
  fn inject(&self, message: SystemMessage) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[derive(Clone)]
pub struct SystemHandle {
  inner: Arc<dyn SystemManager>,
}

impl SystemHandle {
  pub fn new<M: SystemManager + 'static>(manager: M) -> Self {
    Self { inner: Arc::new(manager) }
  }

  pub fn next_button(&self) -> Pin<Box<dyn Future<Output = SystemMessage> + Send + '_>> {
    self.inner.next_button()
  }

  /// Inject a synthetic system event (e.g. remote control over WebSocket)
  pub fn inject(&self, message: SystemMessage) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    self.inner.inject(message)
  }
}

impl fmt::Debug for SystemHandle {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("SystemHandle").finish()
  }
}
