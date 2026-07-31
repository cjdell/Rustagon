use app::platform::{DirEntry, FileType, FsError, LocalFsTrait};
use core::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;

const DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/data");
// const DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../sdk/wasm");

#[derive(Clone)]
pub struct DesktopLocalFs {
  root: PathBuf,
}

impl fmt::Debug for DesktopLocalFs {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("DesktopLocalFs").field("root", &self.root).finish()
  }
}

impl DesktopLocalFs {
  pub fn new() -> Self {
    let root = PathBuf::from(DATA_DIR);
    fs::create_dir_all(&root).ok();
    Self { root }
  }

  fn resolve(&self, name: &str) -> PathBuf {
    let name = name.trim_start_matches('/');
    let path = self.root.join(name);
    // Basic path traversal prevention
    let root_canonical = self.root.canonicalize().unwrap_or_else(|_| self.root.clone());
    let path_canonical = path.canonicalize().unwrap_or(path.clone());
    if !path_canonical.starts_with(&root_canonical) {
      self.root.join("__invalid__")
    } else {
      path
    }
  }

  fn collect_entries(dir: &Path) -> Result<Vec<DirEntry>, FsError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).map_err(|_| FsError::Io)? {
      let entry = entry.map_err(|_| FsError::Io)?;
      let ft = entry.file_type().map_err(|_| FsError::Io)?;
      let name = entry.file_name().to_string_lossy().to_string();
      let size = if ft.is_file() {
        entry.metadata().map(|m| m.len() as u32).unwrap_or(0)
      } else {
        0
      };
      let file_type = if ft.is_dir() { FileType::Dir } else { FileType::File };
      entries.push(DirEntry { name, file_type, size });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
  }
}

impl LocalFsTrait for DesktopLocalFs {
  fn format(&self) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let root = self.root.clone();
    Box::pin(async move {
      fs::remove_dir_all(&root).ok();
      fs::create_dir_all(&root).map_err(|_| FsError::Io)
    })
  }

  fn list_files(&self) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    let root = self.root.clone();
    Box::pin(async move { Self::collect_entries(&root) })
  }

  fn list_dir(&self, path: String) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> {
    let dir = self.resolve(&path);
    Box::pin(async move {
      if !dir.is_dir() {
        return Err(FsError::NotDir);
      }
      Self::collect_entries(&dir)
    })
  }

  fn get_file_size(&self, file_name: String) -> Pin<Box<dyn Future<Output = Result<u32, FsError>> + Send + '_>> {
    let path = self.resolve(&file_name);
    Box::pin(async move {
      fs::metadata(&path).map(|m| m.len() as u32).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound,
        _ => FsError::Io,
      })
    })
  }

  fn read_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    size: u32,
  ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, FsError>> + Send + '_>> {
    let path = self.resolve(&file_name);
    Box::pin(async move {
      let data = fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound,
        _ => FsError::Io,
      })?;
      let pos = pos as usize;
      let size = size as usize;
      if pos >= data.len() {
        return Ok(Vec::new());
      }
      let end = (pos + size).min(data.len());
      Ok(data[pos..end].to_vec())
    })
  }

  fn write_binary_chunk(
    &self,
    file_name: String,
    pos: u32,
    buf: Vec<u8>,
    truncate: bool,
  ) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let path = self.resolve(&file_name);
    Box::pin(async move {
      if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
      }

      let pos = pos as usize;
      if pos == 0 && truncate {
        fs::write(&path, &buf).map_err(|_| FsError::Io)?;
      } else {
        let mut data = if path.exists() {
          fs::read(&path).unwrap_or_default()
        } else {
          Vec::new()
        };
        if pos + buf.len() > data.len() {
          data.resize(pos + buf.len(), 0);
        }
        data[pos..pos + buf.len()].copy_from_slice(&buf);
        if truncate {
          data.truncate(pos + buf.len());
        }
        fs::write(&path, &data).map_err(|_| FsError::Io)?;
      }
      Ok(())
    })
  }

  fn read_text_file(&self, file_name: String) -> Pin<Box<dyn Future<Output = Result<String, FsError>> + Send + '_>> {
    let path = self.resolve(&file_name);
    Box::pin(async move {
      fs::read_to_string(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound,
        _ => FsError::Io,
      })
    })
  }

  fn write_text_file(&self, file_name: String, text: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let path = self.resolve(&file_name);
    Box::pin(async move {
      if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
      }
      fs::write(&path, &text).map_err(|_| FsError::Io)
    })
  }

  fn delete(&self, name: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let path = self.resolve(&name);
    Box::pin(async move {
      if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| match e.kind() {
          std::io::ErrorKind::NotFound => FsError::NotFound,
          _ => FsError::Io,
        })
      } else {
        fs::remove_file(&path).map_err(|e| match e.kind() {
          std::io::ErrorKind::NotFound => FsError::NotFound,
          _ => FsError::Io,
        })
      }
    })
  }

  fn mkdir(&self, dir_name: String) -> Pin<Box<dyn Future<Output = Result<(), FsError>> + Send + '_>> {
    let path = self.resolve(&dir_name);
    Box::pin(async move {
      fs::create_dir(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => FsError::AlreadyExists,
        std::io::ErrorKind::NotFound => FsError::NotFound,
        _ => FsError::Io,
      })
    })
  }

  fn file_exists(&self, name: String) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
    let path = self.resolve(&name);
    Box::pin(async move { path.exists() })
  }

  fn get_file_type(&self, name: String) -> Pin<Box<dyn Future<Output = Result<FileType, FsError>> + Send + '_>> {
    let path = self.resolve(&name);
    Box::pin(async move {
      if path.is_dir() {
        Ok(FileType::Dir)
      } else if path.is_file() {
        Ok(FileType::File)
      } else {
        Err(FsError::NotFound)
      }
    })
  }
}
