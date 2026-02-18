use crate::{
  lib::{HexButton, HexButtonSender, I2cMessage, PowerCtrl, PowerCtrlReceiver},
  utils::{bq25895::Bq25895, i2c::SharedI2cBus, sleep},
};
use aw9523b::{Aw9523b, Dir, OutputMode, Pin, Register};
use core::future::join;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use esp_hal::{
  i2c::master::{Config, I2c},
  peripherals::Peripherals,
  time::Rate,
};
use esp_println::{print, println};
use log::{error, info};

// 0x58
// 0x59
// 0x5A

const I2C_0: u8 = 0x58;
const I2C_1: u8 = 0x59;
const I2C_2: u8 = 0x5A;

// A = 0x5A 6
// B = 0x5A 7
// C = 0x59 0
// D = 0x59 1
// E = 0x59 2
// F = 0x59 3

macro_rules! handle_button {
  ($sender:expr, $pressed:expr, $state:expr, $name:expr) => {
    match ($pressed, $state) {
      (true, false) => {
        // info!("Button {:?} pressed", $name);
        $sender.publish_immediate($name);
        $state = true;
      }
      (false, true) => {
        // info!("Button {} released", "$name");
        $state = false;
      }
      _ => {}
    }
  };
}

#[embassy_executor::task]
pub async fn i2c_task(
  shared_i2c_bus: SharedI2cBus,
  input_sender: HexButtonSender,
  power_ctrl_receiver: PowerCtrlReceiver,
) {
  info!("Starting I2C Task...");

  scan_devices(shared_i2c_bus.clone());

  init_chip(shared_i2c_bus.clone(), I2C_1).await;
  init_chip(shared_i2c_bus.clone(), I2C_2).await;

  join!(
    power_task(shared_i2c_bus.clone(), power_ctrl_receiver),
    button_task(shared_i2c_bus.clone(), shared_i2c_bus.clone(), input_sender)
  )
  .await;
}

async fn init_chip(i2c: impl embedded_hal::i2c::I2c, address: u8) {
  let mut driver = Aw9523b::new(i2c, address);

  // Software reset (register 0x7f = SW_RSTN, value 0x00)
  driver.software_reset().unwrap();

  // Disable interrupts on all pins (registers 0x06-0x07 = INT_P0/INT_P1, value 0xff)
  driver.write_register(Register::INT_P0, 0xff).unwrap();
  driver.write_register(Register::INT_P1, 0xff).unwrap();

  // Set all pins as inputs (registers 0x04-0x05 = CONFIG_P0/CONFIG_P1, value 0xff)
  driver.write_register(Register::CONFIG_P0, 0xff).unwrap();
  driver.write_register(Register::CONFIG_P1, 0xff).unwrap();

  // Set Port0 to Push-Pull mode (register 0x11 = CTL, bit 4 set)
  driver.port0_output_mode(OutputMode::PP).unwrap();

  // Set all LED dimming registers to 0x00 (registers 0x20-0x2F)
  driver.led_set_dimming(Pin::P10, 0x00).unwrap(); // 0x20
  driver.led_set_dimming(Pin::P11, 0x00).unwrap(); // 0x21
  driver.led_set_dimming(Pin::P12, 0x00).unwrap(); // 0x22
  driver.led_set_dimming(Pin::P13, 0x00).unwrap(); // 0x23
  driver.led_set_dimming(Pin::P00, 0x00).unwrap(); // 0x24
  driver.led_set_dimming(Pin::P01, 0x00).unwrap(); // 0x25
  driver.led_set_dimming(Pin::P02, 0x00).unwrap(); // 0x26
  driver.led_set_dimming(Pin::P03, 0x00).unwrap(); // 0x27
  driver.led_set_dimming(Pin::P04, 0x00).unwrap(); // 0x28
  driver.led_set_dimming(Pin::P05, 0x00).unwrap(); // 0x29
  driver.led_set_dimming(Pin::P06, 0x00).unwrap(); // 0x2A
  driver.led_set_dimming(Pin::P07, 0x00).unwrap(); // 0x2B
  driver.led_set_dimming(Pin::P14, 0x00).unwrap(); // 0x2C
  driver.led_set_dimming(Pin::P15, 0x00).unwrap(); // 0x2D
  driver.led_set_dimming(Pin::P16, 0x00).unwrap(); // 0x2E
  driver.led_set_dimming(Pin::P17, 0x00).unwrap(); // 0x2F
}

async fn power_task(i2c: impl embedded_hal::i2c::I2c, power_ctrl_receiver: PowerCtrlReceiver) {
  let mut pmic = Bq25895::new(i2c);

  loop {
    match select(power_ctrl_receiver.receive(), sleep(60_000)).await {
      Either::First(power_ctrl) => match power_ctrl {
        PowerCtrl::PowerOff => {
          pmic.disable_batfet(true).unwrap();
        }
      },
      Either::Second(_) => match pmic.update_state() {
        Ok(state) => {
          println!("Charge: {:?}", state.charge_status);
          println!("Input: {:?}", state.input_status);
          println!("System: {:?}", state.system_status);
          println!("Battery Fault: {:?}", state.battery_fault);
          println!("Boost Fault: {:?}", state.boost_fault);
          println!("Charge Fault: {:?}", state.charge_fault);

          println!(
            "Vbat: {:.2}V, Vsys: {:.2}V, Vbus: {:.2}V, Vboost: {:.2}V, Icharge: {:.2}A",
            state.vbat, state.vsys, state.vbus, state.boostv, state.ichrg
          );
        }
        Err(e) => println!("Read error: {}", e),
      },
    }
  }
}

async fn button_task(
  i2c_d1: impl embedded_hal::i2c::I2c,
  i2c_d2: impl embedded_hal::i2c::I2c,
  sender: HexButtonSender,
) {
  let mut ic_1 = Aw9523b::new(i2c_d1, I2C_1);
  let mut ic_2 = Aw9523b::new(i2c_d2, I2C_2);

  ic_2.set_io_direction(Pin::P06, Dir::INPUT).unwrap(); // A
  ic_2.set_io_direction(Pin::P07, Dir::INPUT).unwrap(); // B
  ic_1.set_io_direction(Pin::P00, Dir::INPUT).unwrap(); // C
  ic_1.set_io_direction(Pin::P01, Dir::INPUT).unwrap(); // D
  ic_1.set_io_direction(Pin::P02, Dir::INPUT).unwrap(); // E
  ic_1.set_io_direction(Pin::P03, Dir::INPUT).unwrap(); // F

  ic_2.pin_gpio_mode(Pin::P02).unwrap();
  // ic_2.pin_gpio_mode(Pin::P04).unwrap();
  // ic_2.pin_gpio_mode(Pin::P05).unwrap();
  ic_2.set_io_direction(Pin::P02, Dir::OUTPUT).unwrap(); // LED_PWR_EN
  // ic_2.set_io_direction(Pin::P04, Dir::OUTPUT).unwrap(); // VBUS_SW
  // ic_2.set_io_direction(Pin::P05, Dir::OUTPUT).unwrap(); // USBSEL
  ic_2.set_pin_high(Pin::P02).unwrap(); // LEDs ON

  ic_2.pin_gpio_mode(Pin::P16).unwrap();
  ic_2.set_io_direction(Pin::P16, Dir::OUTPUT).unwrap();

  ic_2.set_pin_high(Pin::P16).unwrap();
  sleep(50).await;
  ic_2.set_pin_low(Pin::P16).unwrap();
  sleep(50).await;
  ic_2.set_pin_high(Pin::P16).unwrap();

  sender.publish(I2cMessage::DisplayReset).await;

  let mut button_a_down = false;
  let mut button_b_down = false;
  let mut button_c_down = false;
  let mut button_d_down = false;
  let mut button_e_down = false;
  let mut button_f_down = false;

  loop {
    let a_pressed = ic_2.pin_is_low(Pin::P06).unwrap_or_default();
    let b_pressed = ic_2.pin_is_low(Pin::P07).unwrap_or_default();
    let c_pressed = ic_1.pin_is_low(Pin::P00).unwrap_or_default();
    let d_pressed = ic_1.pin_is_low(Pin::P01).unwrap_or_default();
    let e_pressed = ic_1.pin_is_low(Pin::P02).unwrap_or_default();
    let f_pressed = ic_1.pin_is_low(Pin::P03).unwrap_or_default();

    handle_button!(sender, a_pressed, button_a_down, I2cMessage::HexButton(HexButton::A));
    handle_button!(sender, b_pressed, button_b_down, I2cMessage::HexButton(HexButton::B));
    handle_button!(sender, c_pressed, button_c_down, I2cMessage::HexButton(HexButton::C));
    handle_button!(sender, d_pressed, button_d_down, I2cMessage::HexButton(HexButton::D));
    handle_button!(sender, e_pressed, button_e_down, I2cMessage::HexButton(HexButton::E));
    handle_button!(sender, f_pressed, button_f_down, I2cMessage::HexButton(HexButton::F));

    Timer::after(Duration::from_millis(10)).await;
  }
}

fn scan_devices(mut i2c0: impl embedded_hal::i2c::I2c) {
  info!("Scanning I2C bus...");

  for addr in 0x00..0x80 {
    // Skip reserved addresses (0x00-0x07, 0x78-0x7F)
    // Try to write zero bytes - this just checks for ACK
    match i2c0.write(addr, &[]) {
      Ok(_) => {
        info!("Found device at address 0x{:02X}", addr);
      }
      Err(_) => {
        // Device not present - this is expected for most addresses
        // error!("{err}");
      }
    }
  }

  info!("Scan complete");
}

async fn test_i2c() {
  loop {
    for addr in 0x58..=0x5a {
      let p = unsafe { Peripherals::steal() };

      let i2c0 = I2c::new(p.I2C0, Config::default().with_frequency(Rate::from_khz(133)))
        .unwrap()
        .with_sda(p.GPIO45)
        .with_scl(p.GPIO46);

      let mut ic = Aw9523b::new(i2c0, addr);

      if let Ok(_) = ic.id() {
        // info!("==================== aw9523b {:02x} id = {:02x}", addr, id);
        print!("OK ");
      }

      for pin in 0..16 {
        let pin_enum = unsafe { core::mem::transmute::<_, aw9523b::Pin>(pin) };

        if let Err(err) = ic.set_io_direction(pin_enum, aw9523b::Dir::INPUT) {
          info!("set_io_direction error: {err:?}");
        }

        if let Ok(high) = ic.pin_is_high(pin_enum) {
          // info!("Device = {addr:02x} | Pin {pin:02x} = {high}");
          print!("{} ", if high { "X" } else { "0" });
        }
      }
    }

    println!("");
  }
}
