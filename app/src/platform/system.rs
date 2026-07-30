use crate::types::SystemMessage;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::{fmt, future::Future, pin::Pin};

pub trait SystemManager: Send + Sync + fmt::Debug {
  fn next_button(&self) -> Pin<Box<dyn Future<Output = SystemMessage> + Send + '_>>;
  fn inject(&self, message: SystemMessage) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[derive(Clone, Debug)]
pub struct SystemHandle {
  inner: Arc<dyn SystemManager>,
}

impl SystemHandle {
  pub fn new(manager: Arc<dyn SystemManager>) -> Self {
    Self { inner: manager }
  }

  pub async fn next_button(&self) -> SystemMessage {
    self.inner.next_button().await
  }

  pub async fn inject(&self, message: SystemMessage) {
    self.inner.inject(message).await
  }
}
