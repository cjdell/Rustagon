use super::led::{LedHandle, MockLedManager};
use super::traits::Platform;
use alloc::sync::Arc;
use core::fmt;

/// Mock platform implementation for testing without hardware
#[derive(Clone, Debug)]
pub struct MockPlatform {
  led: LedHandle,
}

impl MockPlatform {
  pub fn new() -> Self {
    let led_manager = Arc::new(MockLedManager::new());
    let led = LedHandle::new(led_manager);
    Self { led }
  }
}

impl Default for MockPlatform {
  fn default() -> Self {
    Self::new()
  }
}

impl Platform for MockPlatform {
  fn led(&self) -> LedHandle {
    self.led.clone()
  }
}
