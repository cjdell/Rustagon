use crate::types::{DeviceEvent, HexpansionEvent, HexpansionInfo};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::{fmt, future::Future, pin::Pin};
use embedded_hal::i2c::{ErrorType, I2c, Operation};

pub trait HexpansionManager: Send + Sync + fmt::Debug {
  fn next_event(&self) -> Pin<Box<dyn Future<Output = HexpansionEvent> + Send + '_>>;
  fn try_next_event(&self) -> Option<HexpansionEvent>;
  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)>;
  fn next_device_event(&self) -> Pin<Box<dyn Future<Output = DeviceEvent> + Send + '_>>;
  fn try_next_device_event(&self) -> Option<DeviceEvent>;
}

#[derive(Clone, Debug)]
pub struct HexpansionHandle {
  inner: Arc<dyn HexpansionManager>,
}

impl HexpansionHandle {
  pub fn new(manager: Arc<dyn HexpansionManager>) -> Self {
    Self { inner: manager }
  }

  pub async fn next_event(&self) -> HexpansionEvent {
    self.inner.next_event().await
  }

  pub fn try_next_event(&self) -> Option<HexpansionEvent> {
    self.inner.try_next_event()
  }

  pub fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)> {
    self.inner.current_state()
  }

  pub async fn next_device_event(&self) -> DeviceEvent {
    self.inner.next_device_event().await
  }

  pub fn try_next_device_event(&self) -> Option<DeviceEvent> {
    self.inner.try_next_device_event()
  }
}

/// Error type for `DeviceI2c` operations.
#[derive(Debug)]
pub struct DeviceI2cError;

impl embedded_hal::i2c::Error for DeviceI2cError {
  fn kind(&self) -> embedded_hal::i2c::ErrorKind {
    embedded_hal::i2c::ErrorKind::Other
  }
}

/// Opaque I2C bus handle given to device drivers for their port's I2C bus.
#[derive(Clone, Debug)]
pub struct DeviceI2c {
  inner: Arc<dyn DeviceI2cOps>,
}

impl DeviceI2c {
  pub fn new(inner: Arc<dyn DeviceI2cOps>) -> Self {
    Self { inner }
  }

  /// Combined write-then-read transaction (REPEATED START between).
  pub fn transaction(&self, addr: u8, write_data: &[u8], read_data: &mut [u8]) -> Result<(), DeviceI2cError> {
    self.inner.transaction(addr, write_data, read_data)
  }

  /// Write-only transaction.
  pub fn write(&self, addr: u8, data: &[u8]) -> Result<(), DeviceI2cError> {
    self.inner.write(addr, data)
  }

  /// Read-only transaction.
  pub fn read(&self, addr: u8, data: &mut [u8]) -> Result<(), DeviceI2cError> {
    self.inner.read(addr, data)
  }
}

pub trait DeviceI2cOps: Send + Sync + fmt::Debug {
  fn transaction(&self, addr: u8, write_data: &[u8], read_data: &mut [u8]) -> Result<(), DeviceI2cError>;
  fn write(&self, addr: u8, data: &[u8]) -> Result<(), DeviceI2cError>;
  fn read(&self, addr: u8, data: &mut [u8]) -> Result<(), DeviceI2cError>;
}

/// Resources given to a device driver when it is spawned.
#[derive(Clone, Debug)]
pub struct DeviceIo {
  pub port: u8,
  pub i2c: DeviceI2c,
  pub vid: u16,
  pub pid: u16,
}

// ============================== embedded-hal I2c for DeviceI2c ==============================

impl ErrorType for DeviceI2c {
  type Error = DeviceI2cError;
}

impl I2c for DeviceI2c {
  fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Self::Error> {
    // Combined write+read with REPEATED START (the common pattern for register reads)
    if operations.len() == 2 {
      let (first, rest) = operations.split_at_mut(1);
      let w = &first[0];
      let r = &mut rest[0];
      if let (Operation::Write(w_data), Operation::Read(r_data)) = (w, r) {
        return self.inner.transaction(address, w_data, r_data);
      }
    }
    // Single operation or fallback fallthrough
    for op in operations {
      match op {
        Operation::Read(buffer) => self.inner.read(address, buffer)?,
        Operation::Write(buffer) => self.inner.write(address, buffer)?,
      }
    }
    Ok(())
  }
}
