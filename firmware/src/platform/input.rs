pub use app::platform::input::{InputHandle, InputManager};

use alloc::boxed::Box;
use aw9523b::{Aw9523b, Dir, Pin};
use core::fmt;
use cy8cmbr3116::Cy8cmbr3116;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use crate::d_i2c::*;
use crate::types::HexButton;
use crate::utils::{EventQueue, MaskedI2cBus};

const EVENT_QUEUE_DEPTH: usize = 10;

pub type ButtonEventQueue = EventQueue<HexButton, EVENT_QUEUE_DEPTH>;

#[derive(Clone)]
pub struct HardwareInputManager {
  events: ButtonEventQueue,
}

impl HardwareInputManager {
  pub fn new(spawner: Spawner, sys_bus: MaskedI2cBus, top_bus: MaskedI2cBus) -> Self {
    let events = ButtonEventQueue::new();
    let touch_bus = top_bus.clone();
    spawner.spawn(button_monitoring_task(sys_bus, top_bus, events.clone())).ok();
    spawner.spawn(touch_monitoring_task(touch_bus, events.clone())).ok();
    Self { events }
  }

  /// Clone of the shared button queue, used by keyboard hexpansion drivers so
  /// arrow/enter keys surface as `HexButton` presses indistinguishable from
  /// the physical badge buttons.
  pub fn button_queue(&self) -> ButtonEventQueue {
    self.events.clone()
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

  gpio_i2c_2.set_io_direction(Pin::P06, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P07, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P00, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P01, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P03, Dir::INPUT).unwrap();

  gpio_i2c_3.set_io_direction(Pin::P12, Dir::INPUT).unwrap();
  gpio_i2c_3.set_io_direction(Pin::P11, Dir::INPUT).unwrap();
  gpio_i2c_3.set_io_direction(Pin::P10, Dir::INPUT).unwrap();
  gpio_i2c_3.set_io_direction(Pin::P15, Dir::INPUT).unwrap();
  gpio_i2c_3.set_io_direction(Pin::P14, Dir::INPUT).unwrap();
  gpio_i2c_3.set_io_direction(Pin::P13, Dir::INPUT).unwrap();

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
      events.push(button.released()).await;
      *state = false;
    }
    _ => {}
  }
}

/// Maps the 12 capacitive touch sensors (CS3..CS14) to `HexButton`
/// variants named after the C firmware's TOUCH01-TOUCH12 convention.
/// Sensor CS3 = buttons[0], …, sensor CS14 = buttons[11].
const TOUCH_HEX_MAP: [Option<HexButton>; 12] = [
  Some(HexButton::Touch12),
  Some(HexButton::Touch11),
  Some(HexButton::Touch10),
  Some(HexButton::Touch09),
  Some(HexButton::Touch08),
  Some(HexButton::Touch07),
  Some(HexButton::Touch06),
  Some(HexButton::Touch05),
  Some(HexButton::Touch04),
  Some(HexButton::Touch03),
  Some(HexButton::Touch01), // CS13
  Some(HexButton::Touch02), // CS14
];

#[embassy_executor::task]
async fn touch_monitoring_task(top_bus: MaskedI2cBus, events: ButtonEventQueue) {
  // The CY8CMBR3116 reset pin is AW9523B P07 at I2C address 0x58 on the top bus
  let mut expander = Aw9523b::new(top_bus.clone(), I2C_3);

  // Assert reset (low), wait 10 ms, release (high), wait 50 ms for boot
  expander.set_io_direction(Pin::P07, aw9523b::Dir::OUTPUT).ok();
  expander.set_pin_low(Pin::P07).ok();
  Timer::after(Duration::from_millis(10)).await;
  expander.set_pin_high(Pin::P07).ok();
  Timer::after(Duration::from_millis(50)).await;

  // Initialise the touch controller over I2C
  let mut touch = Cy8cmbr3116::new(top_bus);
  if touch
    .init(|ms| {
      // Can't block in an async context — use a spin-loop for the short
      // delays required by the init sequence.  The init path calls this
      // with 500 ms (NVM save) which is long, but the device NACKs the
      // bus during that period anyway so spinning is acceptable.
      let until = embassy_time::Instant::now() + embassy_time::Duration::from_millis(ms as u64);
      while embassy_time::Instant::now() < until {}
    })
    .is_err()
  {
    // Device not present or init failed — poll anyway; it may appear
    // later (e.g. power-cycled).  Warnings every cycle would be spammy
    // so just silently continue and the poll will fail silently too.
  }

  let mut prev_states = [false; 12];
  loop {
    match touch.poll() {
      Ok(state) => {
        // Sensors CS3..CS14 map to TOUCH12..TOUCH01
        for (i, mapping) in TOUCH_HEX_MAP.iter().enumerate() {
          let sensor = i + 3;
          let touched = state.buttons.is_touched(sensor as u8);
          let prev = &mut prev_states[i];

          match (touched, *prev) {
            (true, false) => {
              if let Some(btn) = mapping {
                events.push(*btn).await;
              }
              *prev = true;
            }
            (false, true) => {
              if let Some(btn) = mapping {
                events.push(btn.released()).await;
              }
              *prev = false;
            }
            _ => {}
          }
        }
      }
      Err(_) => {
        // Bus error or device NACK — retry next cycle
      }
    }
    Timer::after(Duration::from_millis(50)).await;
  }
}
