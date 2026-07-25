use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::fmt;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embassy_time::{Duration, Timer};
use esp_alloc::InternalMemory;
use esp_hal::{
  gpio::{Input, InputConfig, Pull},
  peripherals::GPIO0,
};
use log::info;

use super::traits::*;

pub struct HardwareSystemManager {
  presses: Arc<RwLock<CriticalSectionRawMutex, Vec<SystemMessage>>>,
}

impl HardwareSystemManager {
  pub fn new(spawner: Spawner, pin: GPIO0<'static>) -> Self {
    let presses = Arc::new(RwLock::<CriticalSectionRawMutex, _>::new(Vec::new()));

    spawner.spawn(button_monitoring_task(pin, presses.clone())).ok();

    Self { presses }
  }
}

impl fmt::Debug for HardwareSystemManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareSystemManager").finish()
  }
}

impl SystemManager for HardwareSystemManager {
  fn next_button(&self) -> core::pin::Pin<alloc::boxed::Box<dyn Future<Output = SystemMessage> + Send + '_>> {
    Box::pin(async {
      loop {
        let mut presses = self.presses.write().await;
        if !presses.is_empty() {
          return presses.pop().unwrap();
        }
        drop(presses); // Essential otherwise we hog the lock!

        Timer::after(Duration::from_millis(100)).await;
      }
    })
  }
}

#[embassy_executor::task]
async fn button_monitoring_task(pin: GPIO0<'static>, presses: Arc<RwLock<CriticalSectionRawMutex, Vec<SystemMessage>>>) {
  let mut input = Input::new(pin, InputConfig::default().with_pull(Pull::Up));

  loop {
    input.wait_for_falling_edge().await;
    info!("Boot pin pressed!");

    let mut presses = presses.write().await;
    presses.push(SystemMessage::BootButton);
  }
}
