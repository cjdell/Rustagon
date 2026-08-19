use alloc::sync::Arc;
use core::cell::RefCell;
use embassy_sync::blocking_mutex::{Mutex, raw::NoopRawMutex};
use embedded_hal::i2c::Operation;
use esp_hal::Blocking;

type SharedI2cBus = Arc<Mutex<NoopRawMutex, RefCell<esp_hal::i2c::master::I2c<'static, Blocking>>>>;

pub struct MultiplexedI2cBus {
  i2c: SharedI2cBus,
}

impl MultiplexedI2cBus {
  pub const TOP_BUS: u8 = 0b00000001;
  pub const HX1_BUS: u8 = 0b00000010;
  pub const HX2_BUS: u8 = 0b00000100;
  pub const HX3_BUS: u8 = 0b00001000;
  pub const HX4_BUS: u8 = 0b00010000;
  pub const HX5_BUS: u8 = 0b00100000;
  pub const HX6_BUS: u8 = 0b01000000;
  pub const SYS_BUS: u8 = 0b10000000;

  // The NoopRawMutex bus is !Send + !Sync, but all I2C access is serialized by
  // the single-core executor, so the wrapper types may cross task boundaries.
  #[allow(clippy::arc_with_non_send_sync)]
  pub fn new(i2c: esp_hal::i2c::master::I2c<'static, Blocking>) -> Self {
    Self {
      i2c: Arc::new(Mutex::new(RefCell::new(i2c))),
    }
  }

  pub fn new_masked_i2c_bus(&self, mux_bits: u8) -> MaskedI2cBus {
    MaskedI2cBus::new(self.i2c.clone(), mux_bits)
  }
}

#[derive(Clone)]
pub struct MaskedI2cBus {
  i2c: SharedI2cBus,
  mux_bits: u8,
}

// Safety: MaskedI2cBus is Safe to Send and Sync because:
// - The Arc<Mutex<>> protects concurrent access to the I2C bus
// - Even though NoopRawMutex is not Sync, the mutex ensures safety
// - In a single-threaded executor context (which we use), this is safe
unsafe impl Send for MaskedI2cBus {}
unsafe impl Sync for MaskedI2cBus {}

impl MaskedI2cBus {
  pub const MUX_ADDR: u8 = 0x77;

  fn new(i2c: SharedI2cBus, mux_bits: u8) -> Self {
    Self { i2c, mux_bits }
  }
}

impl embedded_hal::i2c::ErrorType for MaskedI2cBus {
  type Error = esp_hal::i2c::master::Error;
}

impl embedded_hal::i2c::I2c for MaskedI2cBus {
  fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Self::Error> {
    self.i2c.lock(|i2c| -> Result<(), Self::Error> {
      let mut i2c = i2c.borrow_mut();

      // Set the multiplexer bits to enable the desired channel
      i2c.write(Self::MUX_ADDR, &[self.mux_bits])?;

      // Delegate to the underlying I2C driver's transaction implementation,
      // which properly handles REPEATED START between write and read operations
      // (needed for EEPROM combined read: write mem-addr then read data).
      embedded_hal::i2c::I2c::transaction(&mut *i2c, address, operations)
    })
  }
}
