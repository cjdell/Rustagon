use super::traits::{DisplayError, DisplayManager};
use crate::types::LcdScreen;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

pub struct MockDisplayManager {
  has_state: AtomicBool,
}

impl MockDisplayManager {
  pub fn new() -> Self {
    Self {
      has_state: AtomicBool::new(false),
    }
  }
}

impl Default for MockDisplayManager {
  fn default() -> Self {
    Self::new()
  }
}

impl fmt::Debug for MockDisplayManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockDisplayManager")
      .field("has_state", &self.has_state.load(Ordering::SeqCst))
      .finish()
  }
}

impl Clone for MockDisplayManager {
  fn clone(&self) -> Self {
    Self::new()
  }
}

impl DisplayManager for MockDisplayManager {
  fn signal(&self, _screen: LcdScreen) -> Result<(), DisplayError> {
    self.has_state.store(true, Ordering::SeqCst);
    Ok(())
  }

  fn try_signal(&self, _screen: LcdScreen) -> Result<(), DisplayError> {
    self.has_state.store(true, Ordering::SeqCst);
    Ok(())
  }
}
