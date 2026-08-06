use app::platform::display::DisplayHandle;
use app::wasm::host::WasmHost;

#[derive(Debug)]
pub struct DesktopWasmHost {
  display: DisplayHandle,
}

impl DesktopWasmHost {
  pub fn new(display: DisplayHandle) -> Self {
    Self { display }
  }
}

impl WasmHost for DesktopWasmHost {
  fn write_stdout(&mut self, text: &str) {
    print!("{text}");
  }

  fn get_millis(&self) -> u64 {
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap()
      .as_millis() as u64
  }

  fn set_lcd_buffer(&mut self, buffer: &[u8]) {
    // The display manager stores the frame in LCD_BUFFER, which the render
    // loop picks up next frame — no blocking on the WASM side.
    let _ = self.display.signal_raw_frame(buffer);
  }

  fn set_gpio(&mut self, pin_number: u32, state: u32) {
    log::debug!("set_gpio: pin={pin_number} state={state}");
  }
}
