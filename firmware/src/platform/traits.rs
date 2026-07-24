use super::led::LedHandle;
use core::fmt;

/// Central abstraction for all platform hardware operations
/// Allows swapping between real hardware and mocks without changing application code
pub trait Platform: Clone + Send + Sync + fmt::Debug {
  /// Get the LED manager handle
  fn led(&self) -> LedHandle;
}
