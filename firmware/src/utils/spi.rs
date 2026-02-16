use embassy_time::{Duration, Timer};
use embedded_hal::spi::{Operation, SpiBus};
use esp_hal::{
  Async, Blocking,
  gpio::Output,
  spi::master::{Spi, SpiDma, SpiDmaBus},
};

pub struct SpiExclusiveDevice<'a> {
  pub bus: &'a mut Spi<'a, Blocking>,
  pub cs: Output<'a>,
}

#[derive(Debug)]
pub enum SpiExclusiveDeviceError {
  Unknown,
}

impl<'a> SpiExclusiveDevice<'a> {
  pub fn new(bus: &'a mut Spi<'a, Blocking>, cs: Output<'a>) -> Self {
    Self { bus, cs }
  }
}

impl embedded_hal::spi::Error for SpiExclusiveDeviceError {
  fn kind(&self) -> embedded_hal::spi::ErrorKind {
    embedded_hal::spi::ErrorKind::Other
  }
}

impl<'a> embedded_hal::spi::ErrorType for SpiExclusiveDevice<'a> {
  type Error = SpiExclusiveDeviceError;
}

impl<'a> embedded_hal::spi::SpiDevice for SpiExclusiveDevice<'a> {
  fn transaction(&mut self, operations: &mut [embedded_hal::spi::Operation<'_, u8>]) -> Result<(), Self::Error> {
    self.cs.set_low();

    operations
      .iter_mut()
      .try_for_each(|op| match op {
        Operation::Read(buf) => self.bus.read(buf),
        Operation::Write(buf) => self.bus.write(buf),
        Operation::Transfer(read, write) => Ok(()),
        Operation::TransferInPlace(buf) => self.bus.transfer_in_place(buf),
        Operation::DelayNs(ns) => {
          embassy_time::block_for(embassy_time::Duration::from_nanos(*ns as _));
          Ok(())
        }
      })
      .map_err(|_| SpiExclusiveDeviceError::Unknown)?;

    // On failure, it's important to still flush and deassert CS.
    self.bus.flush().map_err(|_| SpiExclusiveDeviceError::Unknown)?;
    self.cs.set_high();

    Ok(())
  }
}
