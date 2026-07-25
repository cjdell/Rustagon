use alloc::boxed::Box;
use core::fmt;
use embassy_executor::Spawner;
use esp_hal::{
  gpio::{Input, InputConfig, Pull},
  peripherals::GPIO0,
};
use log::info;

use super::traits::*;
use crate::utils::EventQueue;

/// Depth of the pending system event queue. Events beyond this are dropped rather than
/// stalling the GPIO monitoring task.
const EVENT_QUEUE_DEPTH: usize = 8;

type SystemEventQueue = EventQueue<SystemMessage, EVENT_QUEUE_DEPTH>;

pub struct HardwareSystemManager {
  events: SystemEventQueue,
}

impl HardwareSystemManager {
  pub fn new(spawner: Spawner, pin: GPIO0<'static>) -> Self {
    let events = SystemEventQueue::new();

    spawner.spawn(button_monitoring_task(pin, events.clone())).ok();

    Self { events }
  }
}

impl fmt::Debug for HardwareSystemManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareSystemManager").finish()
  }
}

impl SystemManager for HardwareSystemManager {
  fn next_button(&self) -> core::pin::Pin<Box<dyn Future<Output = SystemMessage> + Send + '_>> {
    Box::pin(self.events.next())
  }

  fn inject(&self, message: SystemMessage) -> core::pin::Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    Box::pin(self.events.push(message))
  }
}

#[embassy_executor::task]
async fn button_monitoring_task(pin: GPIO0<'static>, events: SystemEventQueue) {
  let mut input = Input::new(pin, InputConfig::default().with_pull(Pull::Up));

  loop {
    input.wait_for_falling_edge().await;
    info!("Boot pin pressed!");

    // Non-blocking: if nobody is consuming, drop the event rather than back up the ISR task.
    events.try_push(SystemMessage::BootButton);
  }
}
