use aw9523b::{Aw9523b, OutputMode, Pin, Register};
use esp_hal::{
  gpio::{Level, Output, OutputConfig},
  peripherals::GPIO9,
};
use log::info;

pub const I2C_0: u8 = 0x58;
pub const I2C_1: u8 = 0x59;
pub const I2C_2: u8 = 0x5A;

pub fn reset_device(pin: GPIO9<'static>) {
  let mut reset = Output::new(pin, Level::High, OutputConfig::default());
  reset.set_high();
}

pub fn scan_devices(mut i2c0: impl embedded_hal::i2c::I2c) {
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

pub async fn init_gpio(i2c: impl embedded_hal::i2c::I2c, address: u8) {
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
