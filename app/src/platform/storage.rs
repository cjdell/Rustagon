use alloc::sync::Arc;
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
pub struct ConfigHandle<State> {
  inner: Arc<dyn ConfigFileTrait<State>>,
}

impl<State> ConfigHandle<State> {
  pub fn new(inner: Arc<dyn ConfigFileTrait<State>>) -> Self {
    Self { inner }
  }
}

impl<State> Deref for ConfigHandle<State> {
  type Target = dyn ConfigFileTrait<State>;
  fn deref(&self) -> &Self::Target {
    &*self.inner
  }
}
