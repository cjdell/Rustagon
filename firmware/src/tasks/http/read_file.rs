use crate::{platform::StorageHandle, types::*, utils::*};
use alloc::{format, string::String, string::ToString};
use embedded_io_async::Read;
use esp_println::print;
use log::info;
use picoserve::response::{
  IntoResponse,
  chunked::{ChunkWriter, ChunkedResponse, Chunks, ChunksWritten},
};

const CHUNK_SIZE: u32 = 4096;

pub struct ReadFileHandler {
  storage: StorageHandle,
  sender: HttpSender,
}

impl ReadFileHandler {
  pub fn new(storage: StorageHandle, sender: HttpSender) -> Self {
    Self { storage, sender }
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
      match self.storage.get_file_size(file_name.clone()).await {
        Ok(file_size) => file_size as u64,
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
      self.storage.clone(),
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
  storage: StorageHandle,
  sender: HttpSender,
  file_name: String,
  file_size: u64,
}

impl FileChunks {
  pub fn new(storage: StorageHandle, sender: HttpSender, file_name: String, file_size: u64) -> Self {
    Self {
      storage,
      sender,
      file_name,
      file_size,
    }
  }
}

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
    let mut read_bytes = 0u32;

    loop {
      let buffer = {
        match self.storage.read_binary_chunk(self.file_name.clone(), read_bytes, CHUNK_SIZE).await {
          Ok(buffer) => buffer,
          Err(err) => {
            write!(chunk_writer, "Read Error: {err:?}").await.expect("Error writing error!");
            self.sender.send(HttpStatusMessage::Idle).await;
            return chunk_writer.finalize().await;
          }
        }
      };

      chunk_writer.write_chunk(&buffer).await?;
      self.sender.send(HttpStatusMessage::Progress(read_bytes, self.file_size as u32)).await;
      print!(".");

      read_bytes += buffer.len() as u32;

      if read_bytes as u64 == self.file_size {
        break;
      }
    }

    self.sender.send(HttpStatusMessage::Idle).await;
    chunk_writer.finalize().await
  }
}
