use alloc::boxed::Box;
use aw9523b::{Aw9523b, Dir, Pin};
use crate::d_i2c::*;
use core::fmt;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use super::traits::*;
use crate::utils::{EventQueue, MaskedI2cBus};

/// Depth of the pending button event queue.
const EVENT_QUEUE_DEPTH: usize = 10;

type ButtonEventQueue = EventQueue<HexButton, EVENT_QUEUE_DEPTH>;

/// Hardware Input Manager for button press events
/// 
/// Spawns a background task that monitors I2C GPIO expanders for button presses
#[derive(Clone)]
pub struct HardwareInputManager {
  events: ButtonEventQueue,
}

impl HardwareInputManager {
  /// Create a new hardware input manager and spawn the I2C monitoring task
  pub fn new(spawner: Spawner, sys_bus: MaskedI2cBus, top_bus: MaskedI2cBus) -> Self {
    let events = ButtonEventQueue::new();
    spawner.spawn(button_monitoring_task(sys_bus, top_bus, events.clone())).ok();
    Self { events }
  }
}

impl fmt::Debug for HardwareInputManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareInputManager").finish()
  }
}

impl InputManager for HardwareInputManager {
  fn next_button(&self) -> core::pin::Pin<Box<dyn core::future::Future<Output = HexButton> + Send + '_>> {
    Box::pin(self.events.next())
  }

  fn inject_button(&self, button: HexButton) -> core::pin::Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    Box::pin(self.events.push(button))
  }
}

#[embassy_executor::task]
async fn button_monitoring_task(sys_bus: MaskedI2cBus, top_bus: MaskedI2cBus, events: ButtonEventQueue) {
  let mut gpio_i2c_1 = Aw9523b::new(sys_bus.clone(), I2C_1);
  let mut gpio_i2c_2 = Aw9523b::new(sys_bus.clone(), I2C_2);
  let mut gpio_i2c_3 = Aw9523b::new(top_bus, I2C_3);

  gpio_i2c_2.set_io_direction(Pin::P06, Dir::INPUT).unwrap(); // A / Up
  gpio_i2c_2.set_io_direction(Pin::P07, Dir::INPUT).unwrap(); // B / Right
  gpio_i2c_1.set_io_direction(Pin::P00, Dir::INPUT).unwrap(); // C / Fire
  gpio_i2c_1.set_io_direction(Pin::P01, Dir::INPUT).unwrap(); // D / Down
  gpio_i2c_1.set_io_direction(Pin::P03, Dir::INPUT).unwrap(); // F / Left

  gpio_i2c_3.set_io_direction(Pin::P12, Dir::INPUT).unwrap(); // HexA
  gpio_i2c_3.set_io_direction(Pin::P11, Dir::INPUT).unwrap(); // HexB
  gpio_i2c_3.set_io_direction(Pin::P10, Dir::INPUT).unwrap(); // HexC
  gpio_i2c_3.set_io_direction(Pin::P15, Dir::INPUT).unwrap(); // HexD
  gpio_i2c_3.set_io_direction(Pin::P14, Dir::INPUT).unwrap(); // HexE
  gpio_i2c_3.set_io_direction(Pin::P13, Dir::INPUT).unwrap(); // HexF

  let mut button_a_down = false;
  let mut button_b_down = false;
  let mut button_c_down = false;
  let mut button_d_down = false;
  let mut button_f_down = false;

  let mut hex_a_down = false;
  let mut hex_b_down = false;
  let mut hex_c_down = false;
  let mut hex_d_down = false;
  let mut hex_e_down = false;
  let mut hex_f_down = false;

  loop {
    let a_pressed = gpio_i2c_2.pin_is_low(Pin::P06).unwrap_or_default();
    let b_pressed = gpio_i2c_2.pin_is_low(Pin::P07).unwrap_or_default();
    let c_pressed = gpio_i2c_1.pin_is_low(Pin::P00).unwrap_or_default();
    let d_pressed = gpio_i2c_1.pin_is_low(Pin::P01).unwrap_or_default();
    let f_pressed = gpio_i2c_1.pin_is_low(Pin::P03).unwrap_or_default();

    let hex_a_pressed = gpio_i2c_3.pin_is_low(Pin::P12).unwrap_or_default();
    let hex_b_pressed = gpio_i2c_3.pin_is_low(Pin::P11).unwrap_or_default();
    let hex_c_pressed = gpio_i2c_3.pin_is_low(Pin::P10).unwrap_or_default();
    let hex_d_pressed = gpio_i2c_3.pin_is_low(Pin::P15).unwrap_or_default();
    let hex_e_pressed = gpio_i2c_3.pin_is_low(Pin::P14).unwrap_or_default();
    let hex_f_pressed = gpio_i2c_3.pin_is_low(Pin::P13).unwrap_or_default();

    handle_button_press(&events, a_pressed, &mut button_a_down, HexButton::Up).await;
    handle_button_press(&events, b_pressed, &mut button_b_down, HexButton::Right).await;
    handle_button_press(&events, c_pressed, &mut button_c_down, HexButton::Fire).await;
    handle_button_press(&events, d_pressed, &mut button_d_down, HexButton::Down).await;
    handle_button_press(&events, f_pressed, &mut button_f_down, HexButton::Left).await;

    handle_button_press(&events, hex_a_pressed, &mut hex_a_down, HexButton::HexA).await;
    handle_button_press(&events, hex_b_pressed, &mut hex_b_down, HexButton::HexB).await;
    handle_button_press(&events, hex_c_pressed, &mut hex_c_down, HexButton::HexC).await;
    handle_button_press(&events, hex_d_pressed, &mut hex_d_down, HexButton::HexD).await;
    handle_button_press(&events, hex_e_pressed, &mut hex_e_down, HexButton::HexE).await;
    handle_button_press(&events, hex_f_pressed, &mut hex_f_down, HexButton::HexF).await;

    Timer::after(Duration::from_millis(10)).await;
  }
}

async fn handle_button_press(events: &ButtonEventQueue, pressed: bool, state: &mut bool, button: HexButton) {
  match (pressed, *state) {
    (true, false) => {
      events.push(button).await;
      *state = true;
    }
    (false, true) => {
      *state = false;
    }
    _ => {}
  }
}
