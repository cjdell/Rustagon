use alloc::sync::Arc;
use core::fmt;
use core::ops::Deref;

pub use embedded_tools::config::{ConfigFileTrait, StateError};
pub use embedded_tools::local_fs::{DirEntry, FileType, FsError, LocalFsTrait};

#[derive(Clone, Debug)]
pub struct StorageHandle {
  inner: Arc<dyn LocalFsTrait>,
}

impl StorageHandle {
  pub fn new(inner: Arc<dyn LocalFsTrait>) -> Self {
    Self { inner }
  }
}

impl Deref for StorageHandle {
  type Target = dyn LocalFsTrait;

  fn deref(&self) -> &Self::Target {
    &*self.inner
  }
}

#[derive(Clone, Debug)]
pub struct ConfigHandle {
  inner: Arc<dyn ConfigFileTrait<crate::types::DeviceConfig>>,
}

impl ConfigHandle {
  pub fn new(inner: Arc<dyn ConfigFileTrait<crate::types::DeviceConfig>>) -> Self {
    Self { inner }
  }
}

impl Deref for ConfigHandle {
  type Target = dyn ConfigFileTrait<crate::types::DeviceConfig>;

  fn deref(&self) -> &Self::Target {
    &*self.inner
  }
}
