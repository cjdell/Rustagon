use super::traits::PowerManager;
use alloc::boxed::Box;
use core::fmt;
use core::pin::Pin;

#[derive(Debug, Clone)]
pub struct MockPowerManager;

impl MockPowerManager {
  pub fn new() -> Self { Self }
}

impl Default for MockPowerManager { fn default() -> Self { Self::new() } }

impl PowerManager for MockPowerManager {
  fn power_off(&self) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async {})
  }
}
