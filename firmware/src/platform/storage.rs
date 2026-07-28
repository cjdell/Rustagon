pub use app::platform::storage::StorageHandle;
pub use embedded_tools::config::StateError;
pub use embedded_tools::local_fs::{DirEntry, FileType, FsError};

use alloc::sync::Arc;
use app::platform::storage::ConfigHandle as AppConfigHandle;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use esp32s3_embedded_tools::flash::LittleFsFlashStorage;
use esp_storage::FlashStorage as EspFlashStorage;

pub type ConfigHandle = AppConfigHandle<crate::types::DeviceConfig>;

#[derive(Clone)]
pub struct HardwareStorageManager {
  raw_flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>,
  partition_offset: u32,
}

impl core::fmt::Debug for HardwareStorageManager {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_struct("HardwareStorageManager").finish()
  }
}

impl HardwareStorageManager {
  pub fn new(
    raw_flash: Arc<RwLock<CriticalSectionRawMutex, EspFlashStorage<'static>>>,
    partition_offset: u32,
  ) -> Self {
    Self { raw_flash, partition_offset }
  }

  pub async fn format(&self) -> Result<(), FsError> {
    log::info!("Formatting filesystem...");
    let mut storage = LittleFsFlashStorage::new(self.raw_flash.clone(), self.partition_offset);
    embedded_tools::local_fs::LocalFs::format(&mut storage)
  }
}
