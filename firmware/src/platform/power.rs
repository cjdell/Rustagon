pub use app::platform::power::{PowerError, PowerHandle, PowerManager, PowerStatus};

use alloc::boxed::Box;
use alloc::sync::Arc;
use bq25895::Bq25895;
use core::sync::atomic::{AtomicBool, Ordering};
use core::{fmt, future::Future, pin::Pin};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embedded_hal::i2c::I2c;

pub struct HardwarePowerManager<I2C: I2c> {
  bq25895: Arc<RwLock<CriticalSectionRawMutex, Bq25895<I2C>>>,
  initialised: AtomicBool,
}

impl<I2C: I2c> HardwarePowerManager<I2C> {
  pub fn new(i2c: I2C) -> Self {
    let bq25895 = Arc::new(RwLock::new(Bq25895::new(i2c)));
    Self { bq25895, initialised: AtomicBool::new(false) }
  }
}

impl<I2C: I2c> fmt::Debug for HardwarePowerManager<I2C> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwarePowerManager").finish()
  }
}

impl<I2C: I2c + Send + 'static> HardwarePowerManager<I2C> {
  async fn ensure_init(&self) {
    if !self.initialised.load(Ordering::Acquire) {
      log::info!("bq: lazy init");
      let mut bq25895 = self.bq25895.write().await;
      match bq25895.init() {
        Ok(()) => {
          self.initialised.store(true, Ordering::Release);
          log::info!("bq: init OK");
        }
        Err(e) => {
          log::warn!("bq: init failed: {e}");
        }
      }
    }
  }
}

impl<I2C: I2c + Send + 'static> PowerManager for HardwarePowerManager<I2C> {
  fn power_off(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    Box::pin(async {
      let mut bq25895 = self.bq25895.write().await;
      let _ = bq25895.disable_batfet(true);
    })
  }

  fn get_status(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>> {
    Box::pin(async {
      self.ensure_init().await;
      let mut bq25895 = self.bq25895.write().await;
      let s = match bq25895.update_state() {
        Ok(s) => s,
        Err(e) => {
          log::warn!("bq: update_state failed: {e}");
          return PowerStatus {
            vbat_mv: 0, vsys_mv: 0, vbus_mv: 0,
            charge_current_ma: 0, charge_voltage_mv: 0,
            input_current_limit_ma: 0,
            is_charging: false, is_power_present: false,
            battery_fault: false,
          };
        }
      };
      let p = PowerStatus {
        vbat_mv: (s.vbat * 1000.0) as u16,
        vsys_mv: (s.vsys * 1000.0) as u16,
        vbus_mv: (s.vbus * 1000.0) as u16,
        charge_current_ma: s.ichrg as u16,
        charge_voltage_mv: (s.vreg * 1000.0) as u16,
        input_current_limit_ma: s.input_current_limit as u16,
        is_charging: matches!(s.charge_status, bq25895::ChargeStatus::PreCharging | bq25895::ChargeStatus::FastCharging),
        // The BQ25895 STATUS register bits don't reliably indicate VBUS
        // presence on this hardware. Instead, use the VBUS ADC reading.
        is_power_present: s.vbus > 4.0,
        battery_fault: !matches!(s.battery_fault, bq25895::BatteryFault::Normal),
      };
      log::info!("bq: PowerStatus {{ vbat={}mV vsys={}mV vbus={}mV ichrg={}mA ilim={}mA present={} }}",
        p.vbat_mv, p.vsys_mv, p.vbus_mv, p.charge_current_ma, p.input_current_limit_ma, p.is_power_present);
      p
    })
  }
}
