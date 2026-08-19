pub use app::platform::power::{PowerError, PowerHandle, PowerManager, PowerStatus};

use crate::utils::MaskedI2cBus;
use alloc::{boxed::Box, sync::Arc};
use app::utils::WatchedValue;
use bq25895::Bq25895;
use core::{fmt, future::Future, pin::Pin};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embassy_time::{Duration, Timer};

const POWER_POLL_INTERVAL: Duration = Duration::from_millis(2_000);

/// Power manager with a background monitoring work loop (same pattern as the
/// LED manager): a spawned task owns the BQ25895 polling cadence and publishes
/// into `WatchedValue<PowerStatus>`. Callers read the current state via
/// `get_status()` or await the next change via `wait_for_change()` — nobody
/// polls the chip on demand.
pub struct HardwarePowerManager {
  bq25895: Arc<RwLock<CriticalSectionRawMutex, Bq25895<MaskedI2cBus>>>,
  status: WatchedValue<PowerStatus>,
}

impl HardwarePowerManager {
  pub fn new(spawner: &Spawner, i2c: MaskedI2cBus) -> Self {
    let bq25895 = Arc::new(RwLock::new(Bq25895::new(i2c)));
    let status = WatchedValue::new(PowerStatus::default());

    spawner.spawn(power_monitoring_task(bq25895.clone(), status.clone()).expect("spawn power_monitoring_task"));

    Self { bq25895, status }
  }
}

impl fmt::Debug for HardwarePowerManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwarePowerManager").finish()
  }
}

#[embassy_executor::task]
async fn power_monitoring_task(bq25895: Arc<RwLock<CriticalSectionRawMutex, Bq25895<MaskedI2cBus>>>, status: WatchedValue<PowerStatus>) {
  let mut initialised = false;
  let mut last = PowerStatus::default();

  loop {
    if !initialised {
      let mut bq25895 = bq25895.write().await;
      match bq25895.init() {
        Ok(()) => {
          initialised = true;
          log::info!("bq: initialised");
        }
        Err(e) => log::debug!("bq: init failed, retrying: {e}"),
      }
    }

    if initialised {
      let mut bq25895 = bq25895.write().await;
      match bq25895.update_state() {
        Ok(s) => {
          let p = PowerStatus {
            vbat_mv: (s.vbat * 1000.0) as u16,
            vsys_mv: (s.vsys * 1000.0) as u16,
            vbus_mv: (s.vbus * 1000.0) as u16,
            charge_current_ma: s.ichrg as u16,
            charge_voltage_mv: (s.vreg * 1000.0) as u16,
            input_current_limit_ma: s.input_current_limit as u16,
            is_charging: matches!(
              s.charge_status,
              bq25895::ChargeStatus::PreCharging | bq25895::ChargeStatus::FastCharging
            ),
            // The BQ25895 STATUS register bits don't reliably indicate VBUS
            // presence on this hardware. Instead, use the VBUS ADC reading.
            is_power_present: s.vbus > 4.0,
            battery_fault: !matches!(s.battery_fault, bq25895::BatteryFault::Normal),
          };

          // Log transitions, not every poll.
          if p.is_charging != last.is_charging || p.is_power_present != last.is_power_present || p.battery_fault != last.battery_fault {
            log::info!(
              "bq: transition: charging={} power_present={} fault={}",
              p.is_charging,
              p.is_power_present,
              p.battery_fault
            );
          }

          last = p;
          status.set(p).await;
        }
        Err(e) => log::debug!("bq: update_state failed: {e}"),
      }
    }

    Timer::after(POWER_POLL_INTERVAL).await;
  }
}

impl PowerManager for HardwarePowerManager {
  fn power_off(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    Box::pin(async {
      let mut bq25895 = self.bq25895.write().await;
      let _ = bq25895.disable_batfet(true);
    })
  }

  fn get_status(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>> {
    Box::pin(self.status.get())
  }

  fn wait_for_change(&self) -> Pin<Box<dyn Future<Output = PowerStatus> + Send + '_>> {
    self.status.wait_for_change()
  }
}
