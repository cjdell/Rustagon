use bq25895::BqState;
use alloc::sync::Arc;
use alloc::boxed::Box;
use core::fmt;
use core::pin::Pin;

/// Trait for power management operations
/// Async trait for managing device power and charging
pub trait PowerManager: Send + Sync + fmt::Debug {
  /// Get current power/charging status (async)
  fn get_status(&self) -> Pin<Box<dyn core::future::Future<Output = BqState> + Send + '_>>;

  /// Power off the device (async)
  fn power_off(&self) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>>;
}

#[derive(Debug, Clone)]
pub enum PowerError {
  I2cError,
  HardwareError,
}

/// A cloneable reference to a power manager
#[derive(Clone, Debug)]
pub struct PowerHandle {
  manager: Arc<dyn PowerManager>,
}

impl PowerHandle {
  pub fn new(manager: Arc<dyn PowerManager>) -> Self {
    Self { manager }
  }

  pub async fn get_status(&self) -> BqState {
    self.manager.get_status().await
  }

  pub async fn power_off(&self) {
    self.manager.power_off().await
  }
}
