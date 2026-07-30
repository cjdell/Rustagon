/// Platform-specific operations needed by the WASM runtime.
pub trait WasmHost {
    fn write_stdout(&mut self, text: &str);

    fn get_millis(&self) -> u64;

    fn set_lcd_buffer(&mut self, buffer: &[u8]);

    fn set_gpio(&mut self, pin: u32, state: u32);
}
