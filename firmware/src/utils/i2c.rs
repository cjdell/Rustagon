use alloc::sync::Arc;
use core::cell::RefCell;
use embassy_sync::blocking_mutex::{Mutex, raw::NoopRawMutex};
use embedded_hal::i2c::Operation;
use esp_hal::Blocking;
use log::{error, info};

#[derive(Clone)]
pub struct SharedI2cBus {
  i2c: Arc<Mutex<NoopRawMutex, RefCell<esp_hal::i2c::master::I2c<'static, Blocking>>>>,
}

impl SharedI2cBus {
  pub const MUX_ADDR: u8 = 0x77;
  pub const SYS_BUS: u8 = 0b10000000;

  pub fn new(i2c: esp_hal::i2c::master::I2c<'static, Blocking>) -> Self {
    Self {
      i2c: Arc::new(Mutex::new(RefCell::new(i2c))),
    }
  }

  pub fn configure_switch(&self, mux_bits: u8) {
    match self.i2c.lock(|i2c| i2c.borrow_mut().write(Self::MUX_ADDR, &[mux_bits])) {
      Ok(_) => {
        info!("configure_switch: {mux_bits}");
      }
      Err(err) => {
        error!("configure_switch: Error: {err:?}");
      }
    };
  }
}

impl embedded_hal::i2c::ErrorType for SharedI2cBus {
  type Error = esp_hal::i2c::master::Error;
}

impl embedded_hal::i2c::I2c for SharedI2cBus {
  fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Self::Error> {
    self.i2c.lock(|i2c| -> Result<(), Self::Error> {
      let mut i2c = i2c.borrow_mut();

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
