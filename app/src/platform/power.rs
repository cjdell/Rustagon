use alloc::sync::Arc;
use alloc::boxed::Box;
use core::{fmt, future::Future, pin::Pin};

#[derive(Debug, Clone)]
pub enum PowerError {
  I2cError,
}

pub trait PowerManager: Send + Sync + fmt::Debug {
  fn power_off(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[derive(Clone, Debug)]
pub struct PowerHandle {
  inner: Arc<dyn PowerManager>,
}

impl PowerHandle {
  pub fn new(manager: Arc<dyn PowerManager>) -> Self {
    Self { inner: manager }
  }

  pub async fn power_off(&self) {
    self.inner.power_off().await
  }
}
