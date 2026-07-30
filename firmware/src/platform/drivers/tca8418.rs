//! Driver for the TCA8418 keypad scanner.
//!
//! Register map per the `tca8418` crate (0.2.2):
//!   https://crates.io/crates/tca8418
//!
//! Key registers:
//!   0x01 CFG     — AI(7) | GPI_E_CFG(6) | OVR_FLOW_M(5) | INT_CFG(4) |
//!                  OVR_FLOW_IEN(3) | K_LCK_IEN(2) | GPI_IEN(1) | KE_IEN(0)
//!   0x02 INT_STAT — interrupt flags
//!   0x03 KEY_LCK_EC — event count (lower nibble) + lock status
//!   0x04 KEY_EVENT_A — key event FIFO (read pops one event)
//!   0x05-0x0D KEY_EVENT_B..J — remaining FIFO slots
//!   0x1D-0x1F KP_GPIO1-3 — keypad pin assignment

use app::platform::hexpansion::DeviceIo;
use app::types::{DeviceEvent, KeyCode, KeyEventType, KeyboardEvent};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use log::info;

use super::DeviceEventQueue;

const TCA_ADDR: u8 = 0x34;

const REG_CFG: u8 = 0x01;
const REG_KEY_LCK_EC: u8 = 0x03;  // event count (lower nibble)
const REG_KEY_EVENT_A: u8 = 0x04; // FIFO read (NOT 0x60!)
const REG_KP_GPIO1: u8 = 0x1D;
const REG_KP_GPIO2: u8 = 0x1E;
const REG_KP_GPIO3: u8 = 0x1F;

// CFG bits (per tca8418 crate)
const CFG_KE_IEN: u8 = 1 << 0;    // key event interrupt enable
const CFG_AI: u8 = 1 << 7;        // auto-increment for key events

fn event_to_keycode(code: u8) -> Option<KeyCode> {
  match code & 0x3F {
    1 => Some(KeyCode::Escape),
    8 => Some(KeyCode::Backspace),
    9 => Some(KeyCode::Digit0),
    10 => Some(KeyCode::Minus), 11 => Some(KeyCode::Backtick),
    12 => Some(KeyCode::Digit1), 13 => Some(KeyCode::Digit2),
    14 => Some(KeyCode::Digit3), 15 => Some(KeyCode::Digit4),
    16 => Some(KeyCode::Digit5), 17 => Some(KeyCode::Digit6),
    18 => Some(KeyCode::Digit7), 19 => Some(KeyCode::Digit8),
    20 => Some(KeyCode::Digit9),
    21 => Some(KeyCode::Tab),
    22 => Some(KeyCode::Q), 23 => Some(KeyCode::W),
    24 => Some(KeyCode::E), 25 => Some(KeyCode::R),
    26 => Some(KeyCode::T), 27 => Some(KeyCode::Y),
    28 => Some(KeyCode::U), 29 => Some(KeyCode::I),
    30 => Some(KeyCode::O),
    32 => Some(KeyCode::A), 33 => Some(KeyCode::S),
    34 => Some(KeyCode::D), 35 => Some(KeyCode::F),
    36 => Some(KeyCode::G), 37 => Some(KeyCode::H),
    38 => Some(KeyCode::J), 39 => Some(KeyCode::K),
    40 => Some(KeyCode::L),
    41 => Some(KeyCode::Shift), 56 => Some(KeyCode::Shift),
    42 => Some(KeyCode::Z), 43 => Some(KeyCode::X),
    44 => Some(KeyCode::C), 45 => Some(KeyCode::V),
    46 => Some(KeyCode::B), 47 => Some(KeyCode::N),
    48 => Some(KeyCode::M),
    49 => Some(KeyCode::Comma), 50 => Some(KeyCode::Period),
    51 => Some(KeyCode::Left), 52 => Some(KeyCode::Down),
    53 => Some(KeyCode::Right), 55 => Some(KeyCode::Up),
    54 => Some(KeyCode::Slash),
    57 => Some(KeyCode::Semicolon), 58 => Some(KeyCode::Quote),
    59 => Some(KeyCode::Enter), 60 => Some(KeyCode::Equals),
    61 => Some(KeyCode::Ctrl),
    63 => Some(KeyCode::Alt), 68 => Some(KeyCode::Alt),
    64 => Some(KeyCode::Backslash),
    65 | 66 | 67 => Some(KeyCode::Space),
    69 => Some(KeyCode::P), 70 => Some(KeyCode::LBracket),
    80 => Some(KeyCode::RBracket),
    _ => None,
  }
}

pub fn tca8418_driver_factory(io: DeviceIo, queue: DeviceEventQueue, spawner: Spawner) {
  spawner.spawn(tca8418_task(io, queue)).ok();
}

#[embassy_executor::task]
async fn tca8418_task(io: DeviceIo, queue: DeviceEventQueue) {
  let i2c = &io.i2c;
  let port = io.port;

  // Init — per tca8418 crate pattern:
  // 1. Assign pins to keypad matrix via KP_GPIO registers
  // 2. Enable key event interrupts via CFG

  // Configure all rows (R0-R7) and columns (C0-C9) as keypad pins
  check(write_reg(i2c, REG_KP_GPIO1, 0xFF));
  check(write_reg(i2c, REG_KP_GPIO2, 0xFF));
  check(write_reg(i2c, REG_KP_GPIO3, 0x03));

  // Enable key event interrupt + auto-increment.
  // Don't set INT_CFG (bit 4) to avoid 50ms deassertion delay.
  check(write_reg(i2c, REG_CFG, CFG_KE_IEN | CFG_AI));  // = 0x81

  info!("tca8418[{port}]: started");

  loop {
    Timer::after(Duration::from_millis(20)).await;

    // Read event count from KEY_LCK_EC register (lower nibble)
    let count = match read_reg(i2c, REG_KEY_LCK_EC) {
      Ok(v) => v & 0x0F,
      Err(_) => { info!("tca8418[{port}]: I2C error"); return; }
    };

    if count == 0 { continue; }

    info!("tca8418[{port}]: {count} event(s)");

    for i in 0..count.min(10) {
      let event_byte = match read_reg(i2c, REG_KEY_EVENT_A) {
        Ok(b) => b,
        Err(_) => break,
      };
      if event_byte == 0x00 { break; }

      let released = (event_byte & 0x80) != 0;
      let code = event_byte & 0x3F;
      let typ = if released { KeyEventType::Released } else { KeyEventType::Pressed };

      info!("tca8418[{port}]: event[{i}] byte=0x{event_byte:02x} code={code}");

      if let Some(kc) = event_to_keycode(code) {
        queue.try_push(DeviceEvent::Keyboard(KeyboardEvent { port, typ, code: kc }));
      }
    }
  }
}

fn check(_: Result<(), ()>) {}

fn write_reg(i2c: &app::platform::hexpansion::DeviceI2c, reg: u8, val: u8) -> Result<(), ()> {
  i2c.write(TCA_ADDR, &[reg, val])
}

fn read_reg(i2c: &app::platform::hexpansion::DeviceI2c, reg: u8) -> Result<u8, ()> {
  let mut data = [0u8];
  i2c.transaction(TCA_ADDR, &[reg], &mut data)?;
  Ok(data[0])
}
