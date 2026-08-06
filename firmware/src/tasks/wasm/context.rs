use app::wasm::host::WasmHost;
use esp_hal::{
  gpio::{AnyPin, Level, Output},
  time::Instant,
};
use esp_println::print;
use log::warn;

use crate::platform::display::DisplayHandle;

pub struct HardwareWasmHost {
  display: DisplayHandle,
}

impl HardwareWasmHost {
  pub fn new(display: DisplayHandle) -> Self {
    Self { display }
  }
}

impl WasmHost for HardwareWasmHost {
  fn write_stdout(&mut self, text: &str) {
    print!("{text}");
  }

  fn get_millis(&self) -> u64 {
    Instant::now().duration_since_epoch().as_millis()
  }

  fn set_lcd_buffer(&mut self, buffer: &[u8]) {
    // Non-blocking: the frame is copied into a ping-pong slot and the
    // display task (core 0) flushes it to SPI while we keep rendering.
    if let Err(err) = self.display.signal_raw_frame(buffer) {
      warn!("HardwareWasmHost: failed to submit LCD frame: {err:?}");
    }
  }

  fn set_gpio(&mut self, pin_number: u32, state: u32) {
    let pin = unsafe { AnyPin::steal(pin_number.try_into().unwrap()) };
    let mut output = Output::new(pin, Level::High, Default::default());
    output.set_level(if state == 0 { Level::Low } else { Level::High });
  }
}
