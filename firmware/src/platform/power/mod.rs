use alloc::sync::Arc;
use bq25895::{Bq25895, BqState};
use core::fmt;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

#[derive(Clone)]
pub struct PowerManager<I2C: embedded_hal::i2c::I2c> {
  bq25895: Arc<Mutex<CriticalSectionRawMutex, Bq25895<I2C>>>,
}

impl<I2C: embedded_hal::i2c::I2c> fmt::Debug for PowerManager<I2C> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("PowerManager").finish()
  }
}

impl<I2C: embedded_hal::i2c::I2c> PowerManager<I2C> {
  pub fn new(i2c: I2C) -> Self {
    let bq25895 = Arc::new(Mutex::new(Bq25895::new(i2c)));

    Self { bq25895 }
  }

  pub async fn get_status(&self) -> BqState {
    let mut bq25895 = self.bq25895.lock().await;

    bq25895.update_state().unwrap()
  }

  pub async fn power_off(&self) {
    let mut bq25895 = self.bq25895.lock().await;

    bq25895.disable_batfet(true).unwrap()
  }
}
