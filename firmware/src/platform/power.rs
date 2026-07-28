pub use app::platform::power::{PowerError, PowerHandle, PowerManager};

use alloc::boxed::Box;
use alloc::sync::Arc;
use bq25895::Bq25895;
use core::{fmt, future::Future, pin::Pin};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embedded_hal::i2c::I2c;

pub struct HardwarePowerManager<I2C: I2c> {
  bq25895: Arc<RwLock<CriticalSectionRawMutex, Bq25895<I2C>>>,
}

impl<I2C: I2c> HardwarePowerManager<I2C> {
  pub fn new(i2c: I2C) -> Self {
    let bq25895 = Arc::new(RwLock::new(Bq25895::new(i2c)));
    Self { bq25895 }
  }
}

impl<I2C: I2c> fmt::Debug for HardwarePowerManager<I2C> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwarePowerManager").finish()
  }
}

impl<I2C: I2c + Send + 'static> HardwarePowerManager<I2C> {
  pub async fn get_status(&self) -> bq25895::BqState {
    let mut bq25895 = self.bq25895.write().await;
    bq25895.update_state().unwrap()
  }
}

impl<I2C: I2c + Send + 'static> PowerManager for HardwarePowerManager<I2C> {
  fn power_off(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    Box::pin(async {
      let mut bq25895 = self.bq25895.write().await;
      let _ = bq25895.disable_batfet(true);
    })
  }
}
