use embedded_tools::config::{ConfigFileTrait, StateError};
use embedded_tools::local_fs::{DirEntry, FileType, FsError, LocalFsTrait};
use alloc::{boxed::Box, format, string::String, sync::Arc, vec::Vec};
use core::{fmt, pin::Pin};

#[derive(Clone, Debug)]
pub struct MockStorageManager;

impl MockStorageManager {
  pub fn new() -> Self {
    Self
  }
}

impl Default for MockStorageManager {
  fn default() -> Self {
    Self::new()
  }
}

impl LocalFsTrait for MockStorageManager {
  fn format(&self) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }

  fn list_files(&self) -> Pin<Box<dyn core::future::Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    Box::pin(async { Ok(Vec::new()) })
  }

  fn list_dir(&self, _path: String) -> Pin<Box<dyn core::future::Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    Box::pin(async { Ok(Vec::new()) })
  }

  fn get_file_size(&self, _file_name: String) -> Pin<Box<dyn core::future::Future<Output = Result<u32, FsError>> + Send + '_>> {
    Box::pin(async { Err(FsError::NotFound) })
  }

  fn read_binary_chunk(
    &self,
    _file_name: String,
    _pos: u32,
    _size: u32,
  ) -> Pin<Box<dyn core::future::Future<Output = Result<Vec<u8>, FsError>> + Send + '_>> {
    Box::pin(async { Err(FsError::NotFound) })
  }

  fn write_binary_chunk(
    &self,
    _file_name: String,
    _pos: u32,
    _buf: Vec<u8>,
    _truncate: bool,
  ) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }

  fn read_text_file(&self, _file_name: String) -> Pin<Box<dyn core::future::Future<Output = Result<String, FsError>> + Send + '_>> {
    Box::pin(async { Err(FsError::NotFound) })
  }

  fn write_text_file(&self, _file_name: String, _text: String) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }

  fn delete(&self, _name: String) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }

  fn mkdir(&self, _dir_name: String) -> Pin<Box<dyn core::future::Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }

  fn file_exists(&self, _name: String) -> Pin<Box<dyn core::future::Future<Output = bool> + Send + '_>> {
    Box::pin(async { false })
  }

  fn get_file_type(&self, _name: String) -> Pin<Box<dyn core::future::Future<Output = Result<FileType, FsError>> + Send + '_>> {
    Box::pin(async { Err(FsError::NotFound) })
  }
}

#[derive(Clone, Debug)]
pub struct MockConfigManager;

impl MockConfigManager {
  pub fn new() -> Self {
    Self
  }
}

impl Default for MockConfigManager {
  fn default() -> Self {
    Self::new()
  }
}

impl ConfigFileTrait<crate::types::DeviceConfig> for MockConfigManager {
  fn get_json(&self) -> Pin<Box<dyn core::future::Future<Output = Result<String, StateError>> + Send + '_>> {
    let cfg = crate::types::DeviceConfig::default();
    Box::pin(async move { serde_json::to_string(&cfg).map_err(|e| StateError::Error(format!("{e:?}"))) })
  }

  fn set_json(&self, _json: Vec<u8>) -> Pin<Box<dyn core::future::Future<Output = Result<(), StateError>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }

  fn get_data(&self) -> Pin<Box<dyn core::future::Future<Output = crate::types::DeviceConfig> + Send + '_>> {
    Box::pin(async { crate::types::DeviceConfig::default() })
  }

  fn set_data(&self, _new_state: crate::types::DeviceConfig) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async {})
  }

  fn save(&self) -> Pin<Box<dyn core::future::Future<Output = Result<(), StateError>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }
}
