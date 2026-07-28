use super::alloc::allocate_http_buffer;
use crate::platform::StorageHandle;
use crate::types::HttpSender;
use crate::types::HttpStatusMessage;
use alloc::{format, vec::Vec};
use picoserve::{
  io::Read,
  response::IntoResponse,
  routing::RequestHandlerService,
};
use serde::Serialize;

const CHUNK_SIZE: usize = 4096;

pub struct WriteFileHandler {
  storage: StorageHandle,
  sender: HttpSender,
}

impl WriteFileHandler {
  pub fn new(storage: StorageHandle, sender: HttpSender) -> Self {
    Self { storage, sender }
  }
}

impl RequestHandlerService<()> for WriteFileHandler {
  async fn call_request_handler_service<R: Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    mut request: picoserve::request::Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let query = request.parts.query().unwrap().try_into_string::<50>().unwrap();
    let file_name = query.replace("file=", "");
    let file_size = request.body_connection.content_length();

    let mut reader = request.body_connection.body().reader();

    let mut buffer = allocate_http_buffer(CHUNK_SIZE);

    let mut written_bytes: usize = 0;

    loop {
      let mut chunk_bytes = 0usize;

      loop {
        let read_bytes = reader.read(&mut buffer[chunk_bytes..]).await?;
        chunk_bytes += read_bytes as usize;
        if read_bytes == 0 { break; }
      }

      if chunk_bytes == 0 {
        self.sender.send(HttpStatusMessage::Idle).await;
        return format!("Expecting more data: file_size={file_size} written_bytes={written_bytes}")
          .write_to(request.body_connection.finalize().await?, response_writer)
          .await;
      }

      let last_chunk = file_size <= written_bytes + chunk_bytes;

      if let Err(err) = self.storage.write_binary_chunk(
        file_name.clone(),
        written_bytes as u32,
        buffer[..chunk_bytes].to_vec(),
        last_chunk,
      ).await {
        self.sender.send(HttpStatusMessage::Idle).await;
        return format!("Write Error: {err:?}")
          .write_to(request.body_connection.finalize().await?, response_writer)
          .await;
      }

      self.sender.send(HttpStatusMessage::Progress(written_bytes as u32, file_size as u32)).await;

      written_bytes += chunk_bytes;

      if last_chunk { break; }
    }

    let connection = request.body_connection.finalize().await?;
    self.sender.send(HttpStatusMessage::Idle).await;

    #[derive(Serialize)]
    struct ResponseJson { pub written_bytes: usize }

    super::common::json_response_fn(&serde_json::to_string(&ResponseJson { written_bytes }).unwrap())
      .write_to(connection, response_writer)
      .await
  }
}
