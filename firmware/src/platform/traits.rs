use super::input::InputHandle;
use super::led::LedHandle;
use super::power::PowerHandle;
use super::system::SystemHandle;
use super::wifi::WiFiHandle;
use core::fmt;

/// Central abstraction for all platform hardware operations
/// Allows swapping between real hardware and mocks without changing application code
pub trait Platform: Clone + Send + Sync + fmt::Debug {
  /// Get the LED manager handle
  fn led_manager(&self) -> LedHandle;

  /// Get the power manager handle
  fn power_manager(&self) -> PowerHandle;

  /// Get the WiFi manager handle
  fn wifi_manager(&self) -> WiFiHandle;

  /// Get the input manager handle
  fn input_manager(&self) -> InputHandle;

  fn system_manager(&self) -> SystemHandle;
}
