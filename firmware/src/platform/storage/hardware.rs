use crate::utils::cpu_guard::CpuGuard;
use embedded_tools::local_fs::{DirEntry, FileType, FsError, LocalFs, LocalFsTrait};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use esp32s3_embedded_tools::flash::LittleFsFlashStorage;
use esp_hal::{peripherals::CPU_CTRL, system::CpuControl};
use esp_storage::FlashStorage as EspFlashStorage;
use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use core::{fmt, pin::Pin};
use log::info;

#[derive(Clone)]
pub struct HardwareStorageManager {
  inner: LocalFs<LittleFsFlashStorage>,
  raw_flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>,
  partition_offset: u32,
}

impl fmt::Debug for HardwareStorageManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareStorageManager").finish()
  }
}

impl HardwareStorageManager {
  pub fn new(
    inner: LocalFs<LittleFsFlashStorage>,
    raw_flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>,
    partition_offset: u32,
  ) -> Self {
    Self { inner, raw_flash, partition_offset }
  }
}

/// Wrap an async FS operation so the second core is parked for its
/// duration. Only needed for operations that write to flash.
async fn with_write_guard<F, T>(op: F) -> T
where
  F: Future<Output = T>,
{
  let mut cpu_ctrl = unsafe { CpuControl::new(CPU_CTRL::steal()) };
  let _guard = CpuGuard::new(&mut cpu_ctrl);
  op.await
}

impl LocalFsTrait for HardwareStorageManager {
  fn format(&self) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    let raw_flash = self.raw_flash.clone();
    let partition_offset = self.partition_offset;

    Box::pin(async move {
      info!("Formatting filesystem...");
      let mut storage = LittleFsFlashStorage::new(raw_flash, partition_offset);
      LocalFs::format(&mut storage)
    })
  }

  fn list_files(&self) -> Pin<Box<dyn core::future::Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    Box::pin(self.inner.list_files())
  }

  fn list_dir(&self, path: String) -> Pin<Box<dyn core::future::Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    Box::pin(self.inner.list_dir(path))
  }

  fn get_file_size(&self, file_name: String) -> Pin<Box<dyn core::future::Future<Output = Result<u32, FsError>> + Send + '_>> {
    Box::pin(self.inner.get_file_size(file_name))
  }

  fn read_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    size: u32,
  ) -> Pin<Box<dyn core::future::Future<Output = Result<Vec<u8>, FsError>> + Send + '_>> {
    Box::pin(self.inner.read_binary_chunk(file_name, pos, size))
  }

  fn write_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    buf: Vec<u8>,
    truncate: bool,
  ) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(with_write_guard(self.inner.write_binary_chunk(file_name, pos, buf, truncate)))
  }

  fn read_text_file(&self, file_name: String) -> Pin<Box<dyn core::future::Future<Output = Result<String, FsError>> + Send + '_>> {
    Box::pin(self.inner.read_text_file(file_name))
  }

  fn write_text_file(
    &self,
    file_name: String,
    text: String,
  ) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(with_write_guard(self.inner.write_text_file(file_name, text)))
  }

  fn delete(&self, name: String) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(with_write_guard(self.inner.delete(name)))
  }

  fn mkdir(&self, dir_name: String) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(with_write_guard(self.inner.mkdir(dir_name)))
  }

  fn file_exists(&self, name: String) -> Pin<Box<dyn core::future::Future<Output = bool> + Send + '_>> {
    Box::pin(self.inner.file_exists(name))
  }

  fn get_file_type(&self, name: String) -> Pin<Box<dyn core::future::Future<Output = Result<FileType, FsError>> + Send + '_>> {
    Box::pin(self.inner.get_file_type(name))
  }
}
