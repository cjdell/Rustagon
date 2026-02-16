use crate::{
  lib::{HttpSender, HttpStatusMessage},
  utils::local_fs::LocalFs,
};
use alloc::{
  format,
  string::{String, ToString},
};
use embedded_io_async::Read;
use esp_println::print;
use esp_storage::FlashStorage;
use log::info;
use picoserve::response::{
  IntoResponse,
  chunked::{ChunkWriter, ChunkedResponse, Chunks, ChunksWritten},
};

pub struct ReadFileHandler {
  local_fs: LocalFs,
  sender: HttpSender,
}

impl ReadFileHandler {
  pub fn new(local_fs: LocalFs, sender: HttpSender) -> Self {
    Self { local_fs, sender }
  }
}

impl picoserve::routing::RequestHandlerService<()> for ReadFileHandler {
  async fn call_request_handler_service<R: Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    request: picoserve::request::Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let query = request.parts.query().unwrap().try_into_string::<50>().unwrap();

    let file_name = query.replace("file=", "");

    let file_size = {
      match self.local_fs.get_file_size(&file_name) {
        Ok(file_size) => file_size,
        Err(err) => {
          return format!("Read File Size Error: {err:?}")
            .write_to(request.body_connection.finalize().await?, response_writer)
            .await;
        }
      }
    };

    info!("Read file: {} {}", file_name, file_size);

    let connection = request.body_connection.finalize().await?;

    ChunkedResponse::new(FileChunks::new(
      self.local_fs.clone(),
      self.sender,
      file_name,
      file_size,
    ))
    .into_response()
    .with_headers([
      ("Access-Control-Allow-Origin", "*"),
      ("Content-Length", &file_size.to_string()),
    ])
    .write_to(connection, response_writer)
    .await
  }
}

struct FileChunks {
  local_fs: LocalFs,
  sender: HttpSender,
  file_name: String,
  file_size: u64,
}

impl FileChunks {
  pub fn new(local_fs: LocalFs, sender: HttpSender, file_name: String, file_size: u64) -> Self {
    Self {
      local_fs,
      sender,
      file_name,
      file_size,
    }
  }
}

// pub fn with<R>(f: impl FnOnce(CriticalSection) -> R) -> R {
//   // Helper for making sure `release` is called even if `f` panics.
//   struct Guard {
//     state: critical_section::RestoreState,
//   }

//   impl Drop for Guard {
//     #[inline(always)]
//     fn drop(&mut self) {
//       unsafe { critical_section::release(self.state) }
//     }
//   }

//   let state = unsafe { critical_section::acquire() };
//   let _guard = Guard { state };

//   unsafe { f(CriticalSection::new()) }
// }

impl Chunks for FileChunks {
  fn content_type(&self) -> &'static str {
    if self.file_name.to_lowercase().ends_with(".txt") {
      "text/plain"
    } else if self.file_name.to_lowercase().ends_with(".jsn") {
      "application/json"
    } else {
      "application/octet-stream"
    }
  }

  async fn write_chunks<W: picoserve::io::Write>(
    self,
    mut chunk_writer: ChunkWriter<W>,
  ) -> Result<ChunksWritten, W::Error> {
    // let mut flash = FlashStorage::new(unsafe { Peripherals::steal() }.FLASH);

    // let local_fs = match LocalFs::new(&mut flash) {
    //   Ok(fs) => fs,
    //   Err(err) => {
    //     write!(chunk_writer, "LocalFs Init Error: {err:?}").await.expect("Error writing error!");
    //     return chunk_writer.finalize().await;
    //   }
    // };

    // struct Guard {
    //   state: critical_section::RestoreState,
    // }

    // print!("acquire acquire acquire acquire acquire acquire acquire acquire");
    // let state = unsafe { critical_section::acquire() };

    // let _guard = Guard { state };

    // impl Drop for Guard {
    //   #[inline(always)]
    //   fn drop(&mut self) {
    //     unsafe {
    //       print!("release release release release release release release release");
    //       critical_section::release(self.state)
    //     }
    //   }
    // }

    // let cs = unsafe { critical_section::CriticalSection::new() };

    // let mut file = {
    //   match local_fs.open_file(cs, &self.file_name) {
    //     Ok(file) => file,
    //     Err(err) => {
    //       write!(chunk_writer, "Open Error: {err:?}")
    //         .await
    //         .expect("Error writing error!");
    //       return chunk_writer.finalize().await;
    //     }
    //   }
    // };

    // let mut buffer = Vec::new_in(ExternalMemory);
    // buffer.resize(FlashStorage::SECTOR_SIZE as usize, 0);

    let mut read_bytes = 0u64;

    loop {
      let buffer = {
        match self.local_fs.read_binary_chunk(&self.file_name, read_bytes, FlashStorage::SECTOR_SIZE as u64) {
          Ok(buffer) => buffer,
          Err(err) => {
            write!(chunk_writer, "Read Error: {err:?}").await.expect("Error writing error!");
            self.sender.send(HttpStatusMessage::None).await;
            return chunk_writer.finalize().await;
          }
        }
      };

      chunk_writer.write_chunk(&buffer).await?;
      self.sender.send(HttpStatusMessage::Progress(read_bytes as u32, self.file_size as u32)).await;
      print!(".");

      read_bytes += buffer.len() as u64;

      if read_bytes == self.file_size {
        break;
      }
    }

    self.sender.send(HttpStatusMessage::None).await;
    chunk_writer.finalize().await
  }
}
