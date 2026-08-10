//! Driver for the KeebDeck keyboard hexpansion using the `tca8418` crate.
//!
//! Hardware: https://github.com/emfcamp/Keebdexpansion
//!
//! The `tca8418` crate handles all register-level configuration and event
//! reading. Navigation keys (arrows, Enter) are unified into `HexButton`
//! presses on the shared button queue so apps cannot tell them apart from
//! the physical badge buttons; all other keys are reported as
//! `DeviceEvent::Keyboard`.

use app::platform::hexpansion::DeviceIo;
use app::types::{DeviceEvent, KeyCode, KeyEventType, KeyboardEvent};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use log::{debug, info};
use tca8418::{PinMask, Tca8418};

use super::{ButtonEventQueue, DeviceEventQueue};

/// Map a TCA8418 key number (1-80) to our KeyCode.
fn key_number_to_keycode(key: u8) -> Option<KeyCode> {
  match key {
    1 => Some(KeyCode::Escape),
    8 => Some(KeyCode::Backspace),
    9 => Some(KeyCode::Digit0),
    10 => Some(KeyCode::Minus),
    11 => Some(KeyCode::Backtick),
    12 => Some(KeyCode::Digit1),
    13 => Some(KeyCode::Digit2),
    14 => Some(KeyCode::Digit3),
    15 => Some(KeyCode::Digit4),
    16 => Some(KeyCode::Digit5),
    17 => Some(KeyCode::Digit6),
    18 => Some(KeyCode::Digit7),
    19 => Some(KeyCode::Digit8),
    20 => Some(KeyCode::Digit9),
    21 => Some(KeyCode::Tab),
    22 => Some(KeyCode::Q),
    23 => Some(KeyCode::W),
    24 => Some(KeyCode::E),
    25 => Some(KeyCode::R),
    26 => Some(KeyCode::T),
    27 => Some(KeyCode::Y),
    28 => Some(KeyCode::U),
    29 => Some(KeyCode::I),
    30 => Some(KeyCode::O),
    32 => Some(KeyCode::A),
    33 => Some(KeyCode::S),
    34 => Some(KeyCode::D),
    35 => Some(KeyCode::F),
    36 => Some(KeyCode::G),
    37 => Some(KeyCode::H),
    38 => Some(KeyCode::J),
    39 => Some(KeyCode::K),
    40 => Some(KeyCode::L),
    41 => Some(KeyCode::Shift),
    42 => Some(KeyCode::Z),
    43 => Some(KeyCode::X),
    44 => Some(KeyCode::C),
    45 => Some(KeyCode::V),
    46 => Some(KeyCode::B),
    47 => Some(KeyCode::N),
    48 => Some(KeyCode::M),
    49 => Some(KeyCode::Comma),
    50 => Some(KeyCode::Period),
    51 => Some(KeyCode::Left),
    52 => Some(KeyCode::Down),
    53 => Some(KeyCode::Right),
    54 => Some(KeyCode::Slash),
    55 => Some(KeyCode::Up),
    56 => Some(KeyCode::Shift),
    57 => Some(KeyCode::Semicolon),
    58 => Some(KeyCode::Quote),
    59 => Some(KeyCode::Enter),
    60 => Some(KeyCode::Equals),
    61 => Some(KeyCode::Ctrl),
    63 => Some(KeyCode::Alt),
    68 => Some(KeyCode::Alt),
    64 => Some(KeyCode::Backslash),
    65 | 66 | 67 => Some(KeyCode::Space),
    69 => Some(KeyCode::P),
    70 => Some(KeyCode::LBracket),
    80 => Some(KeyCode::RBracket),
    _ => None,
  }
}

pub fn tca8418_driver_factory(io: DeviceIo, queue: DeviceEventQueue, buttons: ButtonEventQueue, spawner: Spawner) {
  // A stale task from a previous insertion may still be winding down, in which
  // case spawning fails with Busy — that's fine, just drop the new one.
  if let Ok(token) = tca8418_task(io, queue, buttons) {
    spawner.spawn(token);
  }
}

#[embassy_executor::task]
async fn tca8418_task(io: DeviceIo, queue: DeviceEventQueue, buttons: ButtonEventQueue) {
  let port = io.port;
  let mut kp = Tca8418::new(io.i2c);

  // Assign rows R0-R7 and columns C0-C9 as keypad matrix
  let pins = PinMask::rows(0xFF) | PinMask::cols(0x03FF);
  if let Err(e) = kp.configure_keypad(pins) {
    info!("tca8418[{port}]: configure_keypad failed: {e:?}");
    return;
  }

  // Enable key event interrupts
  if let Err(e) = kp.enable_key_event_interrupt(true) {
    info!("tca8418[{port}]: enable interrupt failed: {e:?}");
    return;
  }

  info!("tca8418[{port}]: started");

  let mut i2c_failures: u32 = 0;

  loop {
    Timer::after(Duration::from_millis(20)).await;

    // Drain all pending events. If I2C fails repeatedly, the hexpansion
    // was likely removed — stop the task.
    let events = match kp.events() {
      Ok(e) => {
        i2c_failures = 0;
        e
      }
      Err(_) => {
        i2c_failures += 1;
        if i2c_failures >= 3 {
          info!("tca8418[{port}]: I2C errors — hexpansion removed, stopping");
          return;
        }
        continue;
      }
    };

    for event in events {
      let typ = if event.pressed {
        KeyEventType::Pressed
      } else {
        KeyEventType::Released
      };

      // pressed_keypad() returns Some for press events, released_keypad() for release
      if let Some(key) = if event.pressed {
        event.pressed_keypad()
      } else {
        event.released_keypad()
      } {
        let key_num = key.get_key_number();
        debug!("tca8418[{port}]: key {key_num} {typ:?}");

        if let Some(kc) = key_number_to_keycode(key_num) {
          debug!("tca8418[{port}]: -> {kc:?}");
          // Navigation keys (arrows, Enter) surface as HexButton presses on the
          // shared button queue, indistinguishable from physical badge buttons.
          if let Some(hex) = kc.to_hex_button() {
            let button = if event.pressed { hex } else { hex.released() };
            buttons.try_push(button);
          } else {
            queue.try_push(DeviceEvent::Keyboard(KeyboardEvent { port, typ, code: kc }));
          }
        }
      }
    }
  }
}
