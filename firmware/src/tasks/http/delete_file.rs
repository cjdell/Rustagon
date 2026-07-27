use crate::platform::StorageHandle;
use alloc::format;
use embedded_io_async::Read;
use picoserve::response::IntoResponse;

pub struct DeleteFileHandler {
  storage: StorageHandle,
}

impl DeleteFileHandler {
  pub(crate) fn new(storage: StorageHandle) -> Self {
    Self { storage }
  }
}

impl picoserve::routing::RequestHandlerService<()> for DeleteFileHandler {
  async fn call_request_handler_service<R: Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    request: picoserve::request::Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let query = request.parts.query().unwrap().try_into_string::<50>().unwrap();

    let file_name = query.replace("file=", "");

    if let Err(err) = self.storage.delete(file_name.clone()).await {
      return format!("Delete Error: {err:?}")
        .write_to(request.body_connection.finalize().await?, response_writer)
        .await;
    }

    format!("Deleted: {file_name}\r\n").write_to(request.body_connection.finalize().await?, response_writer).await
  }
}
