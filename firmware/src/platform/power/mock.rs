use super::traits::PowerManager;
use alloc::boxed::Box;
use bq25895::{BqState, ChargeStatus, InputStatus, SystemStatus, BatteryFault, BoostFault, ChargeFault};
use core::fmt;
use core::pin::Pin;

/// Mock power manager for testing without hardware
#[derive(Debug, Clone)]
pub struct MockPowerManager {
  // Can add state tracking here later if needed
}

impl MockPowerManager {
  pub fn new() -> Self {
    Self {}
  }
}

impl Default for MockPowerManager {
  fn default() -> Self {
    Self::new()
  }
}

impl PowerManager for MockPowerManager {
  fn get_status(&self) -> Pin<Box<dyn core::future::Future<Output = BqState> + Send + '_>> {
    Box::pin(async {
      // Return a reasonable default state for testing
      BqState {
        charge_status: ChargeStatus::NotCharging,
        input_status: InputStatus::NoInput,
        system_status: SystemStatus::Normal,
        battery_fault: BatteryFault::Normal,
        boost_fault: BoostFault::Normal,
        charge_fault: ChargeFault::Normal,
        vbat: 3.7,
        vsys: 5.0,
        vbus: 0.0,
        boostv: 0.0,
        input_current_limit: 500.0,
        ichrg: 0.0,
        vreg: 4.4,
        is_ico_optimized: false,
      }
    })
  }

  fn power_off(&self) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async {
      // Just succeed without doing anything
    })
  }
}
