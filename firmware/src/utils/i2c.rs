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
  pub const SYS_BUS: u8 = 0b10000000;

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

      // Set the multiplexer bits to enable channels we want for this instance
      i2c.write(Self::MUX_ADDR, &[self.mux_bits])?;

      for operation in operations {
        match operation {
          Operation::Read(buffer) => i2c.read(address, buffer)?,
          Operation::Write(buffer) => i2c.write(address, buffer)?,
        }
      }

      Ok(())
    })
  }
}
