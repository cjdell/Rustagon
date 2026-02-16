use super::cpu_guard::CpuGuard;
use super::flash_stream::FlashStream;
use alloc::{
  format,
  string::{String, ToString},
  sync::Arc,
  vec::Vec,
};
use core::str::{self, from_utf8};
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embedded_storage::nor_flash::NorFlash;
use esp_alloc::ExternalMemory;
use esp_hal::{peripherals::CPU_CTRL, system::CpuControl};
use esp_println::println;
use esp_storage::FlashStorage;
use fatfs::{FileSystem, FileSystemStats, FormatVolumeOptions, FsOptions, Read, Seek, SeekFrom, Write, format_volume};
use log::{error, info};
use partitions_macro::{partition_offset, partition_size};
use serde::Serialize;

const FS_OFFSET: u64 = partition_offset!("vfs") as u64;
const FS_LENGTH: u64 = partition_size!("vfs") as u64;

#[derive(Clone)]
pub struct LocalFs {
  fs: Arc<Mutex<CriticalSectionRawMutex, FileSystem<FlashStream>>>, // FileSystem using FlashStream
}

#[derive(Serialize, Debug)]
pub struct FileEntry {
  pub name: String,
  pub size: u64,
}

impl LocalFs {
  pub fn make_new_filesystem(flash: &'static mut FlashStorage<'static>) {
    println!("make_new_filesystem");

    let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
    let _cpu_guard = CpuGuard::new(&mut cpu_ctrl);

    critical_section::with(|_cs| {
      let mut flash_stream = FlashStream::new(flash, FS_OFFSET, FS_LENGTH);
      format_volume(&mut flash_stream, FormatVolumeOptions::default().bytes_per_sector(4096)).unwrap();
    });
  }

  pub fn erase_filesystem(flash: &'static mut FlashStorage) {
    println!("erase_filesystem");

    let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
    let _cpu_guard = CpuGuard::new(&mut cpu_ctrl);

    let zeros = [0u8; FlashStorage::SECTOR_SIZE as usize];
    for pos in (FS_OFFSET..FS_OFFSET + FS_LENGTH).step_by(FlashStorage::SECTOR_SIZE as usize) {
      let percent = ((pos - FS_OFFSET) * 100) / FS_LENGTH;
      println!("Erasing: {percent}% ({pos} / {FS_LENGTH})");
      critical_section::with(|_cs| {
        flash.write(pos as u32, &zeros).unwrap();
      });
    }

    println!("erase_filesystem: Complete");
  }

  pub fn new(flash: &'static mut FlashStorage) -> Result<Self, bool> {
    info!("LocalFs.new: {:?} {:?}", FS_OFFSET, FS_LENGTH);

    let flash_stream = FlashStream::new(flash, FS_OFFSET, FS_LENGTH);

    let fs = match FileSystem::new(flash_stream, FsOptions::new()) {
      Ok(fs) => fs,
      Err(err) => {
        error!("LocalFs.new: {:?}", err);

        match err {
          fatfs::Error::CorruptedFileSystem => {
            return Err(true);
          }
          _ => todo!(),
        }
      }
    };

    let fs = Arc::new(Mutex::new(fs));

    Ok(LocalFs { fs })
  }

  pub fn stats(&self) -> Result<FileSystemStats, FsError> {
    critical_section::with(|cs| {
      let fs = self.fs.borrow(cs);

      fs.stats().map_err(|err| FsError::OpenError(format!("{:?}", err)))
    })
  }

  pub fn dir(&self) -> Result<Vec<FileEntry>, FsError> {
    critical_section::with(|cs| {
      let fs = self.fs.borrow(cs);

      let root_dir = fs.root_dir();

      let mut entries = Vec::<FileEntry>::new();

      for file in root_dir.iter() {
        let file = file.map_err(|err| FsError::OpenError(format!("{:?}", err)))?;

        let name: String =
          file.file_name().as_str().try_into().map_err(|err| FsError::OpenError(format!("{:?}", err)))?;

        let size = file.len();

        let entry = FileEntry { name, size };

        entries.push(entry);
      }

      Ok(entries)
    })
  }

  pub fn get_file_size(&self, file_name: &str) -> Result<u64, FsError> {
    critical_section::with(|cs| {
      let fs = self.fs.borrow(cs);

      let root_dir = fs.root_dir();

      for file in root_dir.iter() {
        let file = file.unwrap();

        if file.file_name().eq_ignore_ascii_case(file_name) {
          return Ok(file.len());
        }
      }

      Err(FsError::OpenError("File not found".to_string()))
    })
  }

  pub fn delete_file(&self, file_name: &str) -> Result<(), FsError> {
    println!("delete_file: {file_name}");

    let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
    let _cpu_guard = CpuGuard::new(&mut cpu_ctrl);

    critical_section::with(|cs| {
      let fs = self.fs.borrow(cs);

      let root_dir = fs.root_dir();

      root_dir.remove(file_name).map_err(|err| FsError::OpenError(err.to_string()))?;

      Ok(())
    })
  }

  pub fn read_binary_chunk(&self, file_name: &str, pos: u64, size: u64) -> Result<Vec<u8, ExternalMemory>, FsError> {
    println!("read_binary_chunk: {file_name} {pos} {size}");

    critical_section::with(|cs| {
      let fs = self.fs.borrow(cs);

      let root_dir = fs.root_dir();

      let file_size =
        self.get_file_size(file_name).map_err(|_err| FsError::OpenError("Could not read length".to_string()))?;

      // Prevent underflow / invalid range
      if pos >= file_size {
        return Ok(Vec::new_in(ExternalMemory));
      }

      let len = (file_size - pos).min(size) as usize;
      let mut buf = Vec::new_in(ExternalMemory);
      buf.resize(len, 0u8);

      println!("read_binary_chunk: buf.len={}", buf.len());

      let mut file = root_dir.open_file(file_name).map_err(|err| FsError::OpenError(err.to_string()))?;

      file.seek(SeekFrom::Start(pos)).map_err(|err| FsError::ReadError(err.to_string()))?;

      match file.read_exact(&mut buf) {
        Ok(()) => {}
        Err(err) => match err {
          fatfs::Error::UnexpectedEof => {
            println!("EOF");
          }
          _ => return Err(FsError::ReadError(err.to_string())),
        },
      }

      Ok(buf)
    })
  }

  pub fn write_binary_chunk(&self, file_name: &str, pos: u64, buf: &[u8], finalise: bool) -> Result<(), FsError> {
    println!("write_binary_chunk: {file_name} pos={pos} len={}", buf.len());

    let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
    let _cpu_guard = CpuGuard::new(&mut cpu_ctrl);

    critical_section::with(|cs| {
      let fs = self.fs.borrow(cs);

      let root_dir = fs.root_dir();

      // Open file for writing — create if needed, truncate at position
      let mut file = root_dir.create_file(file_name).map_err(|err| FsError::OpenError(err.to_string()))?;

      // Seek to position
      file.seek(SeekFrom::Start(pos)).map_err(|err| FsError::WriteError(err.to_string()))?;

      file.write_all(&buf).map_err(|err| FsError::WriteError(err.to_string()))?;

      if finalise {
        file.truncate().map_err(|err| FsError::WriteError(err.to_string()))?;
      }

      file.flush().map_err(|err| FsError::WriteError(err.to_string()))?;

      Ok(())
    })
  }

  // TODO: 32KB hard limit for now
  pub fn read_text_file(&self, file_name: &str) -> Result<String, FsError> {
    Ok(
      from_utf8(&self.read_binary_chunk(file_name, 0, 32 * 1024)?)
        .map_err(|err| FsError::ReadError(err.to_string()))?
        .to_string(),
    )
  }

  pub fn write_text_file(&self, file_name: &str, content: &str) -> Result<(), FsError> {
    let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
    let _cpu_guard = CpuGuard::new(&mut cpu_ctrl);

    critical_section::with(|cs| {
      let fs = self.fs.borrow(cs);

      let root_dir = fs.root_dir();

      let mut file = root_dir.create_file(file_name).map_err(|err| FsError::OpenError(err.to_string()))?;

      let buf = content.as_bytes();

      file.write_all(&buf).map_err(|err| FsError::WriteError(err.to_string()))?;

      file.truncate().map_err(|err| FsError::WriteError(err.to_string()))?;

      file.flush().map_err(|err| FsError::WriteError(err.to_string()))?;

      Ok(())
    })
  }
}

#[derive(Debug)]
pub enum FsError {
  OpenError(String),
  ReadError(String),
  WriteError(String),
}
