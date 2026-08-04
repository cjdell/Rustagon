use crate::alloc_ext::external_vec;
use crate::types::{HttpSender, HttpStatusMessage};
use alloc::vec::Vec;
use picoserve::{
  io::Read,
  request::Request,
  response::{IntoResponse, ResponseWriter},
  routing::RequestHandlerService,
};
use serde::Serialize;

pub struct ReceiveFileHandler {
  sender: HttpSender,
}

impl ReceiveFileHandler {
  pub fn new(sender: HttpSender) -> Self {
    Self { sender }
  }
}

impl RequestHandlerService<()> for ReceiveFileHandler {
  async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    mut request: Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let file_size = request.body_connection.content_length();

    let mut reader = request.body_connection.body().reader();

    let mut buffer = external_vec(file_size);

    let mut received_bytes: usize = 0;

    loop {
      let read_bytes = reader.read(&mut buffer[received_bytes..]).await?;
      received_bytes += read_bytes as usize;
      if read_bytes == 0 {
        break;
      }
      self
        .sender
        .send(HttpStatusMessage::Progress(received_bytes as u32, file_size as u32))
        .await;
    }

    let connection = request.body_connection.finalize().await?;

    self.sender.send(HttpStatusMessage::ReceivedFile(buffer)).await;

    #[derive(Serialize)]
    struct ResponseJson {
      pub received_bytes: usize,
    }

    super::common::json_response_fn(&serde_json::to_string(&ResponseJson { received_bytes }).unwrap())
      .write_to(connection, response_writer)
      .await
  }
}
