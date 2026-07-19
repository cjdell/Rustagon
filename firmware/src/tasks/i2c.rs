use crate::{d_i2c::*, types::*, utils::*};
use aw9523b::{Aw9523b, Dir, Pin};
use core::future::join;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use esp_hal::{
  i2c::master::{Config, I2c},
  peripherals::Peripherals,
  time::Rate,
};
use esp_println::{print, println};
use log::info;

// 0x58
// 0x59
// 0x5A

// A = 0x5A 6
// B = 0x5A 7
// C = 0x59 0
// D = 0x59 1
// E = 0x59 2
// F = 0x59 3

// EPIN_ND_A = (2, 12)
// EPIN_ND_B = (2, 13)
// EPIN_ND_C = (1, 8)
// EPIN_ND_D = (1, 9)
// EPIN_ND_E = (1, 10)
// EPIN_ND_F = (1, 11)

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
pub async fn i2c_task(sys_bus: MaskedI2cBus, input_sender: HexButtonSender, power_ctrl_receiver: PowerCtrlReceiver) {
  info!("Starting I2C Task...");

  join!(
    power_task(sys_bus.clone(), power_ctrl_receiver),
    button_task(sys_bus.clone(), sys_bus.clone(), input_sender)
  )
  .await;
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

async fn button_task(i2c_d1: impl embedded_hal::i2c::I2c, i2c_d2: impl embedded_hal::i2c::I2c, sender: HexButtonSender) {
  let mut gpio_i2c_1 = Aw9523b::new(i2c_d1, I2C_1);
  let mut gpio_i2c_2 = Aw9523b::new(i2c_d2, I2C_2);

  gpio_i2c_2.set_io_direction(Pin::P06, Dir::INPUT).unwrap(); // A / Up
  gpio_i2c_2.set_io_direction(Pin::P07, Dir::INPUT).unwrap(); // B / Right
  gpio_i2c_1.set_io_direction(Pin::P00, Dir::INPUT).unwrap(); // C / Fire
  gpio_i2c_1.set_io_direction(Pin::P01, Dir::INPUT).unwrap(); // D / Down
  gpio_i2c_1.set_io_direction(Pin::P02, Dir::INPUT).unwrap(); // E / ????
  gpio_i2c_1.set_io_direction(Pin::P03, Dir::INPUT).unwrap(); // F / Left

  gpio_i2c_2.set_io_direction(Pin::P10, Dir::INPUT).unwrap(); // HexA
  gpio_i2c_2.set_io_direction(Pin::P15, Dir::INPUT).unwrap(); // HexB
  gpio_i2c_1.set_io_direction(Pin::P10, Dir::INPUT).unwrap(); // HexC
  gpio_i2c_1.set_io_direction(Pin::P11, Dir::INPUT).unwrap(); // HexD
  gpio_i2c_1.set_io_direction(Pin::P12, Dir::INPUT).unwrap(); // HexE
  gpio_i2c_1.set_io_direction(Pin::P13, Dir::INPUT).unwrap(); // HexF

  let mut button_a_down = false;
  let mut button_b_down = false;
  let mut button_c_down = false;
  let mut button_d_down = false;
  let mut button_e_down = false;
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
    let e_pressed = gpio_i2c_1.pin_is_low(Pin::P02).unwrap_or_default();
    let f_pressed = gpio_i2c_1.pin_is_low(Pin::P03).unwrap_or_default();

    let hex_a_pressed = gpio_i2c_2.pin_is_low(Pin::P10).unwrap_or_default();
    let hex_b_pressed = gpio_i2c_2.pin_is_low(Pin::P15).unwrap_or_default();
    let hex_c_pressed = gpio_i2c_1.pin_is_low(Pin::P10).unwrap_or_default();
    let hex_d_pressed = gpio_i2c_1.pin_is_low(Pin::P11).unwrap_or_default();
    let hex_e_pressed = gpio_i2c_1.pin_is_low(Pin::P12).unwrap_or_default();
    let hex_f_pressed = gpio_i2c_1.pin_is_low(Pin::P13).unwrap_or_default();

    handle_button!(sender, a_pressed, button_a_down, I2cMessage::HexButton(HexButton::A));
    handle_button!(sender, b_pressed, button_b_down, I2cMessage::HexButton(HexButton::B));
    handle_button!(sender, c_pressed, button_c_down, I2cMessage::HexButton(HexButton::C));
    handle_button!(sender, d_pressed, button_d_down, I2cMessage::HexButton(HexButton::D));
    handle_button!(sender, e_pressed, button_e_down, I2cMessage::HexButton(HexButton::E));
    handle_button!(sender, f_pressed, button_f_down, I2cMessage::HexButton(HexButton::F));

    handle_button!(sender, hex_a_pressed, hex_a_down, I2cMessage::HexButton(HexButton::HexA));
    handle_button!(sender, hex_b_pressed, hex_b_down, I2cMessage::HexButton(HexButton::HexB));
    handle_button!(sender, hex_c_pressed, hex_c_down, I2cMessage::HexButton(HexButton::HexC));
    handle_button!(sender, hex_d_pressed, hex_d_down, I2cMessage::HexButton(HexButton::HexD));
    handle_button!(sender, hex_e_pressed, hex_e_down, I2cMessage::HexButton(HexButton::HexE));
    handle_button!(sender, hex_f_pressed, hex_f_down, I2cMessage::HexButton(HexButton::HexF));

    Timer::after(Duration::from_millis(10)).await;
  }
}

async fn test_i2c() {
  loop {
    for addr in 0x58..=0x5a {
      let p = unsafe { Peripherals::steal() };

      let i2c0 = I2c::new(p.I2C0, Config::default().with_frequency(Rate::from_khz(133))).unwrap().with_sda(p.GPIO45).with_scl(p.GPIO46);

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
