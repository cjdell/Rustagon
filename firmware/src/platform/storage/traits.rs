pub use app::platform::storage::{ConfigHandle as AppConfigHandle, StorageHandle};
pub use embedded_tools::config::{ConfigFileTrait, StateError};
pub use embedded_tools::local_fs::{DirEntry, FileType, FsError, LocalFsTrait};

pub type ConfigHandle = AppConfigHandle<crate::types::DeviceConfig>;
