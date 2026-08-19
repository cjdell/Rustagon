use alloc::sync::Arc;
use block_device::BlockDevice;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embedded_storage::ReadStorage;
use embedded_storage::nor_flash::NorFlash;
use esp_storage::FlashStorage as EspFlashStorage;

const BLOCK_SIZE: usize = 4 * 1024;
const FLASH_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct FlashStorage {
  flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>,
}

impl FlashStorage {
  pub fn new(flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>) -> Self {
    Self { flash }
  }
}

impl BlockDevice for FlashStorage {
  const BLOCK_SIZE: u32 = BLOCK_SIZE as u32;

  type Error = FlashError;

  fn read(&self, buf: &mut [u8], address: usize, number_of_blocks: usize) -> Result<(), Self::Error> {
    if !address.is_multiple_of(Self::BLOCK_SIZE as usize) {
      return Err(FlashError::AddressNotAligned);
    }

    let required_len = number_of_blocks * Self::BLOCK_SIZE as usize;
    if buf.len() < required_len {
      return Err(FlashError::BufferSizeIncorrect);
    }

    let mut flash = self.flash.try_write().map_err(|_| FlashError::FlashBusy)?;

    ReadStorage::read(&mut *flash, address as u32, &mut buf[..required_len]).map_err(|_| FlashError::ReadFailed)
  }

  fn write(&self, buf: &[u8], address: usize, number_of_blocks: usize) -> Result<(), Self::Error> {
    if !address.is_multiple_of(Self::BLOCK_SIZE as usize) {
      return Err(FlashError::AddressNotAligned);
    }

    if buf.len() > number_of_blocks * Self::BLOCK_SIZE as usize {
      return Err(FlashError::BufferSizeIncorrect);
    }

    if buf.is_empty() {
      return Ok(());
    }

    let mut flash = self.flash.try_write().map_err(|_| FlashError::FlashBusy)?;

    embedded_storage::Storage::write(&mut *flash, address as u32, buf).map_err(|_| FlashError::WriteFailed)
  }
}

pub struct LittleFsFlashStorage {
  flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>,
  offset: u32,
}

impl LittleFsFlashStorage {
  pub fn new(flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>, offset: u32) -> Self {
    Self { flash, offset }
  }
}

impl littlefs_rust::Storage for LittleFsFlashStorage {
  fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), littlefs_rust::Error> {
    let block_address = self.offset + block * BLOCK_SIZE as u32;
    let absolute_address = block_address + offset;

    if offset as usize + buf.len() > BLOCK_SIZE {
      return Err(littlefs_rust::Error::Io);
    }

    let flash_end = FLASH_SIZE as u32;
    if absolute_address as usize + buf.len() > flash_end as usize {
      return Err(littlefs_rust::Error::Io);
    }

    let mut flash = self.flash.try_write().map_err(|_| littlefs_rust::Error::Io)?;

    ReadStorage::read(&mut *flash, absolute_address, buf).map_err(|_| littlefs_rust::Error::Io)
  }

  fn write(&mut self, block: u32, offset: u32, data: &[u8]) -> Result<(), littlefs_rust::Error> {
    let block_address = self.offset + block * BLOCK_SIZE as u32;
    let absolute_address = block_address + offset;

    if offset as usize + data.len() > BLOCK_SIZE {
      return Err(littlefs_rust::Error::Io);
    }

    let flash_end = FLASH_SIZE as u32;
    if absolute_address as usize + data.len() > flash_end as usize {
      return Err(littlefs_rust::Error::Io);
    }

    let mut flash = self.flash.try_write().map_err(|_| littlefs_rust::Error::Io)?;

    NorFlash::write(&mut *flash, absolute_address, data).map_err(|_| littlefs_rust::Error::Io)
  }

  fn erase(&mut self, block: u32) -> Result<(), littlefs_rust::Error> {
    let block_address = self.offset + block * BLOCK_SIZE as u32;

    let flash_end = FLASH_SIZE as u32;
    if block_address + BLOCK_SIZE as u32 > flash_end {
      return Err(littlefs_rust::Error::Io);
    }

    let mut flash = self.flash.try_write().map_err(|_| littlefs_rust::Error::Io)?;

    NorFlash::erase(&mut *flash, block_address, block_address + BLOCK_SIZE as u32).map_err(|_| littlefs_rust::Error::Io)
  }

  fn sync(&mut self) -> Result<(), littlefs_rust::Error> {
    defmt::info!("LittleFsFlashStorage.sync()");
    Ok(())
  }
}

#[derive(Debug)]
pub enum FlashError {
  AddressNotAligned,
  BufferSizeIncorrect,
  FlashBusy,
  ReadFailed,
  WriteFailed,
}
