use alloc::boxed::Box;
use alloc::sync::Arc;
use aw9523b::{Aw9523b, Dir, Pin};
use crate::d_i2c::*;
use core::fmt;
use embassy_executor::Spawner;
use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  channel::{Receiver, Sender},
};
use embassy_time::{Duration, Timer};

use super::traits::*;
use crate::utils::MaskedI2cBus;

/// Hardware Input Manager for button press events
/// 
/// Spawns a background task that monitors I2C GPIO expanders for button presses
#[derive(Clone)]
pub struct HardwareInputManager {
  button_rx: Arc<Receiver<'static, CriticalSectionRawMutex, HexButton, 10>>,
}

impl HardwareInputManager {
  /// Create a new hardware input manager and spawn the I2C monitoring task
  pub fn new(
    spawner: Spawner,
    sys_bus: MaskedI2cBus,
    top_bus: MaskedI2cBus,
    button_tx: Sender<'static, CriticalSectionRawMutex, HexButton, 10>,
    button_rx: Receiver<'static, CriticalSectionRawMutex, HexButton, 10>,
  ) -> Self {
    spawner.spawn(button_monitoring_task(sys_bus, top_bus, button_tx)).ok();
    Self {
      button_rx: Arc::new(button_rx),
    }
  }
}

impl fmt::Debug for HardwareInputManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareInputManager").finish()
  }
}

impl InputManager for HardwareInputManager {
  fn next_button(&self) -> core::pin::Pin<Box<dyn core::future::Future<Output = HexButton> + Send + '_>> {
    let rx = self.button_rx.clone();
    Box::pin(async move {
      // Clone the receiver for this specific wait
      let mut rx_clone = (*rx).clone();
      rx_clone.receive().await
    })
  }
}

#[embassy_executor::task]
async fn button_monitoring_task(
  sys_bus: MaskedI2cBus,
  top_bus: MaskedI2cBus,
  sender: Sender<'static, CriticalSectionRawMutex, HexButton, 10>,
) {
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

    handle_button_press(&sender, a_pressed, &mut button_a_down, HexButton::Up).await;
    handle_button_press(&sender, b_pressed, &mut button_b_down, HexButton::Right).await;
    handle_button_press(&sender, c_pressed, &mut button_c_down, HexButton::Fire).await;
    handle_button_press(&sender, d_pressed, &mut button_d_down, HexButton::Down).await;
    handle_button_press(&sender, f_pressed, &mut button_f_down, HexButton::Left).await;

    handle_button_press(&sender, hex_a_pressed, &mut hex_a_down, HexButton::HexA).await;
    handle_button_press(&sender, hex_b_pressed, &mut hex_b_down, HexButton::HexB).await;
    handle_button_press(&sender, hex_c_pressed, &mut hex_c_down, HexButton::HexC).await;
    handle_button_press(&sender, hex_d_pressed, &mut hex_d_down, HexButton::HexD).await;
    handle_button_press(&sender, hex_e_pressed, &mut hex_e_down, HexButton::HexE).await;
    handle_button_press(&sender, hex_f_pressed, &mut hex_f_down, HexButton::HexF).await;

    Timer::after(Duration::from_millis(10)).await;
  }
}

async fn handle_button_press(
  sender: &Sender<'static, CriticalSectionRawMutex, HexButton, 10>,
  pressed: bool,
  state: &mut bool,
  button: HexButton,
) {
  match (pressed, *state) {
    (true, false) => {
      let _ = sender.send(button).await;
      *state = true;
    }
    (false, true) => {
      *state = false;
    }
    _ => {}
  }
}
