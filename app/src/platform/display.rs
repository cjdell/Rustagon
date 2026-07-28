use alloc::sync::Arc;
use core::fmt;
use display_types::LcdScreen;

#[derive(Debug, Clone)]
pub enum DisplayError {
  SignalBusy,
}

pub trait DisplayManager: Send + Sync + fmt::Debug {
  fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError>;
  fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError>;
}

#[derive(Clone, Debug)]
pub struct DisplayHandle {
  inner: Arc<dyn DisplayManager>,
}

impl DisplayHandle {
  pub fn new(manager: Arc<dyn DisplayManager>) -> Self {
    Self { inner: manager }
  }

  pub fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    self.inner.signal(screen)
  }

  pub fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    self.inner.try_signal(screen)
  }
}
