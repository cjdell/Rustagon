pub use app::platform::led::{LedError, LedHandle, LedManager};

use crate::d_i2c::*;
use crate::types::LedRequest;
use crate::utils::{Aw9523bOutputPin, MaskedI2cBus};
use alloc::vec;
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use app::types::LedState;
use aw9523b::Pin;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};
use display_renderer::led_effects::{
  BreatheEffect, ChaseEffect, FireEffect, LedEffect, OffEffect, RainbowEffect, SolidEffect, SparkleEffect, TheaterChaseEffect,
};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Duration, Timer};
use embedded_hal::digital::OutputPin as _;
use esp_hal::time::Instant;
use log::info;
use static_cell::StaticCell;

pub struct HardwareLedManager {
  request_sender: Sender<'static, CriticalSectionRawMutex, LedRequest, 10>,
  initialized: Arc<AtomicBool>,
}

impl HardwareLedManager {
  pub fn new(spawner: &Spawner, sys_bus: MaskedI2cBus) -> Self {
    let initialized = Arc::new(AtomicBool::new(false));
    let initialized_clone = initialized.clone();

    static LED_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, LedRequest, 10>> = StaticCell::new();
    let channel = LED_CHANNEL.init(Channel::new());
    let request_sender = channel.sender();
    let request_receiver = channel.receiver();

    spawner.spawn(led_work_loop_task(sys_bus, request_receiver, initialized_clone)).ok();

    Self {
      request_sender,
      initialized,
    }
  }
}

impl fmt::Debug for HardwareLedManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareLedManager")
      .field("initialized", &self.initialized.load(Ordering::Acquire))
      .finish()
  }
}

impl LedManager for HardwareLedManager {
  fn request(&self, request: LedRequest) -> Result<(), LedError> {
    self.request_sender.try_send(request).map_err(|_| LedError::ChannelFull)
  }
}

#[embassy_executor::task]
async fn led_work_loop_task(
  sys_bus: MaskedI2cBus,
  led_receiver: Receiver<'static, CriticalSectionRawMutex, LedRequest, 10>,
  initialized: Arc<AtomicBool>,
) {
  info!("Starting LED manager work loop...");

  let mut led_power_enable = Aw9523bOutputPin::new(sys_bus, I2C_2, Pin::P02);
  led_power_enable.set_high().unwrap();

  let mut led_service = crate::utils::led_service::LedService::new();

  let mut current_effect: Box<dyn LedEffect> = Box::new(SolidEffect {
    colour: display_types::LedState::new(255, 0, 0),
  });
  let mut counter = 0;

  initialized.store(true, Ordering::Release);

  loop {
    if let Ok(new_request) = led_receiver.try_receive() {
      current_effect = match new_request {
        LedRequest::Off => Box::new(OffEffect),
        LedRequest::Solid(led_state) => Box::new(SolidEffect {
          colour: display_types::LedState::new(led_state.r, led_state.g, led_state.b),
        }),
        LedRequest::Rainbow => Box::new(RainbowEffect::new(0.1)),
        LedRequest::Breathe(led_state) => Box::new(BreatheEffect::new(
          display_types::LedState::new(led_state.r, led_state.g, led_state.b),
          0.001,
        )),
        LedRequest::Chase(led_state) => Box::new(ChaseEffect::new(
          display_types::LedState::new(led_state.r, led_state.g, led_state.b),
          5,
          50,
        )),
        LedRequest::Sparkle(led_state) => Box::new(SparkleEffect::new(
          display_types::LedState::new(led_state.r, led_state.g, led_state.b),
          0.3,
          100,
        )),
        LedRequest::TheaterChase(led_state) => Box::new(TheaterChaseEffect::new(
          display_types::LedState::new(led_state.r, led_state.g, led_state.b),
          3,
          100,
        )),
        LedRequest::Fire => Box::new(FireEffect::new(55, 120, 30)),
      };
    }

    let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();
    let states = current_effect.update_and_render(now_ms);

    let internal_led = if counter % 2 == 0 {
      LedState::new(255, 0, 0)
    } else {
      LedState::new(0, 0, 255)
    };
    let mut converted_states: Vec<LedState> = Vec::with_capacity(states.len());
    for led_state in states.iter() {
      converted_states.push(LedState::new(led_state.r, led_state.g, led_state.b));
    }
    let mut states_vec = Vec::new();
    states_vec.push(vec![internal_led]);
    states_vec.push(converted_states);
    let states = states_vec.concat();

    led_service.send(&states).await;

    Timer::after(Duration::from_millis(10)).await;
    counter += 1;
  }
}
