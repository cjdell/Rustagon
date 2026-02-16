use alloc::{format, string::String};
use core::fmt;
use embedded_storage::{ReadStorage, Storage};
use fatfs::{IoBase, IoError};

#[derive(Debug)]
pub enum FlashStreamError {
  UnexpectedEof,
  WriteZero,
  StorageError(String), // For underlying storage errors without full std::Error
}

impl fmt::Display for FlashStreamError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      FlashStreamError::UnexpectedEof => write!(f, "Unexpected EOF encountered"),
      FlashStreamError::WriteZero => write!(f, "Write zero error"),
      FlashStreamError::StorageError(msg) => write!(f, "Storage error: {}", msg),
    }
  }
}

impl IoError for FlashStreamError {
  fn is_interrupted(&self) -> bool {
    false
  }

  fn new_unexpected_eof_error() -> Self {
    FlashStreamError::UnexpectedEof
  }

  fn new_write_zero_error() -> Self {
    FlashStreamError::WriteZero
  }
}

pub struct FlashStream {
  flash: &'static mut esp_storage::FlashStorage<'static>,
  offset: u64,
  size: u64,
  pos: u64,
}

impl FlashStream {
  pub fn new(flash: &'static mut esp_storage::FlashStorage, offset: u64, size: u64) -> FlashStream {
    FlashStream {
      flash,
      offset,
      size,
      pos: 0,
    }
  }
}

impl IoBase for FlashStream {
  type Error = FlashStreamError;
}

impl fatfs::Seek for FlashStream {
  fn seek(&mut self, pos: fatfs::SeekFrom) -> Result<u64, Self::Error> {
    match pos {
      fatfs::SeekFrom::Start(offset) => self.pos = offset,
      fatfs::SeekFrom::End(offset) => self.pos = ((self.size as i64) + offset) as u64,
      fatfs::SeekFrom::Current(offset) => self.pos = ((self.pos as i64) + offset) as u64,
    };

    Ok(self.pos)
  }
}

impl fatfs::Read for FlashStream {
  fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
    let buf_len = buf.len();

    // info!("READ POS:{} LEN:{}", self.pos, buf_len);

    self
      .flash
      .read((self.offset + self.pos) as u32, buf)
      .map_err(|err| FlashStreamError::StorageError(format!("{:?}", err)))?;

    let bytes_read = buf_len.min((self.size - self.pos) as usize);
    self.pos += bytes_read as u64;

    Ok(bytes_read)
  }
}

impl fatfs::Write for FlashStream {
  fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
    let buf_len = buf.len();

    // info!("WRITE POS:{} LEN:{}", self.pos, buf_len);

    self
      .flash
      .write((self.offset + self.pos) as u32, buf)
      .map_err(|err| FlashStreamError::StorageError(format!("{:?}", err)))?;

    let bytes_written = buf_len.min((self.size - self.pos) as usize);
    self.pos += bytes_written as u64;

    Ok(bytes_written)
  }

  fn flush(&mut self) -> Result<(), Self::Error> {
    // No-op in this implementation
    Ok(())
  }
}
