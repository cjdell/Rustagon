use alloc::boxed::Box;
use alloc::sync::Arc;
use core::{fmt, future::Future, pin::Pin};

#[derive(Debug, Clone)]
pub enum PowerError {
  I2cError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PowerStatus {
  pub vbat_mv: u16,
  pub vsys_mv: u16,
  pub vbus_mv: u16,
  pub charge_current_ma: u16,
  pub charge_voltage_mv: u16,
  pub input_current_limit_ma: u16,
  pub is_charging: bool,
  pub is_power_present: bool,
  pub battery_fault: bool,
}

impl PowerStatus {
  pub fn battery_percent(&self) -> u8 {
    // Li-ion nominal range ~3.0V–4.2V, map 0–100%.
    // Compute in `u32` — `(4200 - 3000) * 100` overflows `u16` and would panic
    // in debug builds (or wrap to a wrong value in release).
    let mv = self.vbat_mv.clamp(3000, 4200) as u32;
    ((mv - 3000) * 100 / (4200 - 3000)) as u8
  }
}

pub trait PowerManager: Send + Sync + fmt::Debug {
  fn power_off(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
  fn get_status(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>>;
  /// Wait for the next power status change and return the new status.
  fn wait_for_change(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>>;
}

#[derive(Clone, Debug)]
pub struct PowerHandle {
  inner: Arc<dyn PowerManager>,
}

impl PowerHandle {
  pub fn new(manager: Arc<dyn PowerManager>) -> Self {
    Self { inner: manager }
  }

  pub async fn power_off(&self) {
    self.inner.power_off().await
  }

  pub async fn get_status(&self) -> PowerStatus {
    self.inner.get_status().await
  }

  pub async fn wait_for_change(&self) -> PowerStatus {
    self.inner.wait_for_change().await
  }
}
