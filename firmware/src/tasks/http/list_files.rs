use crate::platform::StorageHandle;
use alloc::{format, string::String};
use embedded_io_async::Read;
use picoserve::response::{
  IntoResponse,
  chunked::{ChunkWriter, ChunkedResponse, Chunks, ChunksWritten},
};
use serde::Serialize;

#[derive(Serialize)]
struct FileEntry {
  pub name: String,
  pub size: u32,
}

pub struct HandleFileList {
  storage: StorageHandle,
}

impl HandleFileList {
  pub fn new(storage: StorageHandle) -> Self {
    Self { storage }
  }
}

impl picoserve::routing::RequestHandlerService<()> for HandleFileList {
  async fn call_request_handler_service<R: Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    request: picoserve::request::Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let connection = request.body_connection.finalize().await?;

    ChunkedResponse::new(FileListChunks::new(self.storage.clone()))
      .into_response()
      .with_headers([("Access-Control-Allow-Origin", "*")])
      .write_to(connection, response_writer)
      .await
  }
}

struct FileListChunks {
  storage: StorageHandle,
}

impl FileListChunks {
  pub fn new(storage: StorageHandle) -> Self {
    Self { storage }
  }
}

impl Chunks for FileListChunks {
  fn content_type(&self) -> &'static str {
    "application/json"
  }

  async fn write_chunks<W: picoserve::io::Write>(
    self,
    mut chunk_writer: ChunkWriter<W>,
  ) -> Result<ChunksWritten, W::Error> {
    let entries = match self.storage.list_files().await {
      Ok(entries) => entries,
      Err(err) => {
        chunk_writer.write_chunk(format!("Dir Error: {err:?}").as_bytes()).await?;

        return chunk_writer.finalize().await;
      }
    };

    chunk_writer.write_chunk(b"[").await?;

    for (i, entry) in entries.iter().enumerate() {
      let file_entry = FileEntry {
        name: entry.name.clone(),
        size: entry.size,
      };

      let json = match serde_json::to_string::<FileEntry>(&file_entry) {
        Ok(json) => json,
        Err(err) => {
          panic!("JSON Error: {err:?}");
        }
      };

      chunk_writer.write_chunk(json.as_bytes()).await?;

      if i < entries.len() - 1 {
        chunk_writer.write_chunk(b",").await?;
      }
    }

    chunk_writer.write_chunk(b"]").await?;

    chunk_writer.finalize().await
  }
}
