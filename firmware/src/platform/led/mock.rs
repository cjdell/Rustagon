use super::traits::{LedError, LedManager};
use crate::types::LedRequest;
use core::fmt;

/// Mock LED manager for testing without hardware
#[derive(Debug, Clone)]
pub struct MockLedManager {
  // Can add state tracking here later if needed
}

impl MockLedManager {
  pub fn new() -> Self {
    Self {}
  }
}

impl Default for MockLedManager {
  fn default() -> Self {
    Self::new()
  }
}

impl LedManager for MockLedManager {
  fn request(&self, _request: LedRequest) -> Result<(), LedError> {
    // Just succeed without doing anything
    Ok(())
  }
}
