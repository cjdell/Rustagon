use crate::types::LcdScreen;
use alloc::sync::Arc;
use core::fmt;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

pub type LcdSignal = Signal<CriticalSectionRawMutex, LcdScreen>;

pub trait DisplayManager: Send + Sync + fmt::Debug {
  fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError>;

  fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError>;
}

#[derive(Debug, Clone)]
pub enum DisplayError {
  SignalBusy,
}

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

impl Clone for DisplayHandle {
  fn clone(&self) -> Self {
    Self {
      inner: self.inner.clone(),
    }
  }
}

impl fmt::Debug for DisplayHandle {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("DisplayHandle").finish()
  }
}
