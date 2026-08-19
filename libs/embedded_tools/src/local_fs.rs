use alloc::{boxed::Box, string::String, string::ToString, sync::Arc, vec, vec::Vec};
use core::{
  fmt,
  pin::Pin,
  str::{from_utf8, Utf8Error},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use littlefs_rust::{Config, Filesystem, OpenFlags, SeekFrom, Storage};
use log::info;

pub const BLOCK_SIZE: u32 = 4 * 1024;
pub const FILESYSTEM_SIZE: u32 = 1024 * 1024;

/// Cap for `read_text_file`. Its callers read small text documents (device
/// config JSON, SSH key PEMs), so a bounded allocation in PSRAM is deliberate.
/// Larger files should use `read_binary_chunk`, which supports ranged reads.
pub const MAX_TEXT_FILE_SIZE: u32 = 32 * 1024;

/// Type of a filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
  File,
  Dir,
}

/// A single entry returned by directory listing operations.
#[derive(Debug, Clone)]
pub struct DirEntry {
  pub name: String,
  pub file_type: FileType,
  pub size: u32,
}

/// Object-safe filesystem operations trait.
pub trait LocalFsTrait: Send + Sync + fmt::Debug {
  fn format(&self) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>>;

  fn list_files(&self) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>>;

  fn list_dir(&self, path: String) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>>;

  fn get_file_size(&self, file_name: String) -> Pin<Box<dyn Future<Output = Result<u32, FsError>> + Send + '_>>;

  fn read_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    size: u32,
  ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, FsError>> + Send + '_>>;

  fn write_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    buf: Vec<u8>,
    truncate: bool,
  ) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>>;

  fn read_text_file(&self, file_name: String) -> Pin<Box<dyn Future<Output = Result<String, FsError>> + Send + '_>>;

  fn write_text_file(&self, file_name: String, text: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>>;

  fn delete(&self, name: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>>;

  fn mkdir(&self, dir_name: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>>;

  fn file_exists(&self, name: String) -> Pin<Box<dyn Future<Output = bool> + Send + '_>>;

  fn get_file_type(&self, name: String) -> Pin<Box<dyn Future<Output = Result<FileType, FsError>> + Send + '_>>;
}

/// Wrapper that asserts `Filesystem` is `Send`.
///
/// # Safety
/// `Filesystem` wraps the littlefs C library which stores internal state via raw pointers.
/// It is not automatically `Send` because raw pointers are `!Send`.
/// However, all access to this type is serialized through the `Mutex` in `LocalFs`,
/// and littlefs itself is designed to be reentrant when each instance has exclusive access.
/// The thread-safety concern is that the same `Filesystem` is never accessed concurrently
/// from multiple threads - the `Mutex` guarantees this.
struct SendFilesystem<S: Storage>(Filesystem<S>);

unsafe impl<S: Storage> Send for SendFilesystem<S> {}

impl<S: Storage> core::ops::Deref for SendFilesystem<S> {
  type Target = Filesystem<S>;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl<S: Storage> core::ops::DerefMut for SendFilesystem<S> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

pub struct LocalFs<STORAGE: Storage> {
  fs: Arc<Mutex<CriticalSectionRawMutex, SendFilesystem<STORAGE>>>,
}

impl<STORAGE: Storage> fmt::Debug for LocalFs<STORAGE> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("LocalFs").finish()
  }
}

impl<STORAGE: Storage> Clone for LocalFs<STORAGE> {
  fn clone(&self) -> Self {
    Self { fs: self.fs.clone() }
  }
}

impl<STORAGE: Storage> LocalFs<STORAGE> {
  pub fn format(io: &mut STORAGE) -> Result<(), FsError> {
    let config = Config::new(BLOCK_SIZE, FILESYSTEM_SIZE / BLOCK_SIZE);

    Filesystem::format(io, &config).map_err(FsError::from)?;

    Ok(())
  }

  pub fn new(io: STORAGE) -> Result<Self, FsError> {
    let config = Config::new(BLOCK_SIZE, FILESYSTEM_SIZE / BLOCK_SIZE);

    let fs = Filesystem::mount(io, config).map_err(|(err, _)| FsError::from(err))?;
    let fs = Arc::new(Mutex::new(SendFilesystem(fs)));

    Ok(Self { fs })
  }
}

impl<STORAGE: Storage + 'static> LocalFsTrait for LocalFs<STORAGE> {
  fn format(&self) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async { Err(FsError::Io) })
  }

  fn list_files(&self) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    let path = "/".to_string();
    Box::pin(async move { self.list_dir(path).await })
  }

  fn list_dir(&self, path: String) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    Box::pin(async move {
      let fs = &*self.fs.lock().await;
      let list = fs.list_dir(&path).map_err(FsError::from)?;
      Ok(list.into_iter().map(DirEntry::from).collect())
    })
  }

  fn get_file_size(&self, file_name: String) -> Pin<Box<dyn Future<Output = Result<u32, FsError>> + Send + '_>> {
    Box::pin(async move {
      let fs = &*self.fs.lock().await;
      let metadata = fs.stat(&file_name).map_err(FsError::from)?;
      Ok(metadata.size)
    })
  }

  fn read_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    size: u32,
  ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, FsError>> + Send + '_>> {
    Box::pin(async move {
      if pos == 0 {
        info!("==== read_binary_chunk: {} {} {}", file_name, pos, size);
      }

      let mut buf = vec![0u8; size as usize];

      let fs = &*self.fs.lock().await;

      let file = fs.open(&file_name, OpenFlags::READ).map_err(FsError::from)?;

      file.seek(SeekFrom::Start(pos)).map_err(FsError::from)?;

      let bytes_read = file.read(&mut buf).map_err(FsError::from)?;

      buf.resize(bytes_read as usize, 0u8);

      Ok(buf)
    })
  }

  fn write_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    buf: Vec<u8>,
    truncate: bool,
  ) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async move {
      if pos == 0 {
        info!("==== write_binary_chunk: {} {}", file_name, pos);
      }

      let fs = &*self.fs.lock().await;

      let file = fs.open(&file_name, OpenFlags::WRITE | OpenFlags::CREATE).map_err(FsError::from)?;

      file.seek(SeekFrom::Start(pos)).map_err(FsError::from)?;

      file.write(&buf).map_err(FsError::from)?;

      if truncate {
        file.truncate(pos + buf.len() as u32).map_err(FsError::from)?;
      }

      Ok(())
    })
  }

  fn read_text_file(&self, file_name: String) -> Pin<Box<dyn Future<Output = Result<String, FsError>> + Send + '_>> {
    Box::pin(async move {
      let chunk = self.read_binary_chunk(file_name, 0, MAX_TEXT_FILE_SIZE).await?;
      let text = from_utf8(&chunk).map_err(FsError::Decoding)?;
      Ok(text.to_string())
    })
  }

  fn write_text_file(&self, file_name: String, text: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async move {
      let buf = text.as_bytes().to_vec();
      self.write_binary_chunk(file_name, 0, buf, true).await
    })
  }

  fn delete(&self, name: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async move {
      let fs = &*self.fs.lock().await;
      fs.remove(&name).map_err(FsError::from)
    })
  }

  fn mkdir(&self, dir_name: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    Box::pin(async move {
      let fs = &*self.fs.lock().await;
      fs.mkdir(&dir_name).map_err(FsError::from)
    })
  }

  fn file_exists(&self, name: String) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
    Box::pin(async move {
      let fs = &*self.fs.lock().await;
      fs.exists(&name)
    })
  }

  fn get_file_type(&self, name: String) -> Pin<Box<dyn Future<Output = Result<FileType, FsError>> + Send + '_>> {
    Box::pin(async move {
      let fs = &*self.fs.lock().await;
      let metadata = fs.stat(&name).map_err(FsError::from)?;
      Ok(FileType::from(metadata.file_type))
    })
  }
}

impl From<littlefs_rust::FileType> for FileType {
  fn from(file_type: littlefs_rust::FileType) -> Self {
    match file_type {
      littlefs_rust::FileType::File => FileType::File,
      littlefs_rust::FileType::Dir => FileType::Dir,
    }
  }
}

impl From<littlefs_rust::DirEntry> for DirEntry {
  fn from(entry: littlefs_rust::DirEntry) -> Self {
    DirEntry {
      name: entry.name,
      file_type: FileType::from(entry.file_type),
      size: entry.size,
    }
  }
}

impl From<littlefs_rust::Error> for FsError {
  fn from(err: littlefs_rust::Error) -> Self {
    match err {
      littlefs_rust::Error::Io => FsError::Io,
      littlefs_rust::Error::Corrupt => FsError::Corrupt,
      littlefs_rust::Error::NoEntry => FsError::NotFound,
      littlefs_rust::Error::Exists => FsError::AlreadyExists,
      littlefs_rust::Error::NotDir => FsError::NotDir,
      littlefs_rust::Error::IsDir => FsError::IsDir,
      littlefs_rust::Error::NotEmpty => FsError::NotEmpty,
      littlefs_rust::Error::Invalid => FsError::Invalid,
      littlefs_rust::Error::NoSpace => FsError::NoSpace,
      littlefs_rust::Error::NoMemory => FsError::NoMemory,
      littlefs_rust::Error::NoAttribute => FsError::NoAttribute,
      littlefs_rust::Error::NameTooLong => FsError::NameTooLong,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
  NotFound,
  AlreadyExists,
  IsDir,
  NotDir,
  NotEmpty,
  Io,
  Corrupt,
  Invalid,
  NoSpace,
  NoMemory,
  NameTooLong,
  NoAttribute,
  Decoding(Utf8Error),
}
