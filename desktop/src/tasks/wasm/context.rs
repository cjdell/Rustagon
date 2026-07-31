use app::wasm::host::WasmHost;
use std::sync::Mutex;

pub static LCD_BUFFER: Mutex<Option<Vec<u8>>> = Mutex::new(None);

#[derive(Debug)]
pub struct DesktopWasmHost;

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
    if let Ok(mut lcd) = LCD_BUFFER.lock() {
      *lcd = Some(buffer.to_vec());
    }
  }

  fn set_gpio(&mut self, pin_number: u32, state: u32) {
    log::debug!("set_gpio: pin={pin_number} state={state}");
  }
}
