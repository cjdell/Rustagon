use crate::platform::Platform;
use crate::types::OtaError;
use alloc::format;
use picoserve::{
  ResponseSent, io::Read,
  request::Request,
  response::{IntoResponse, ResponseWriter},
  routing::RequestHandlerService,
};

pub struct OtaUpdateHandler<P: Platform> {
  platform: P,
}

impl<P: Platform> OtaUpdateHandler<P> {
  pub fn new(platform: P) -> Self {
    Self { platform }
  }
}

impl<P: Platform> RequestHandlerService<()> for OtaUpdateHandler<P> {
  async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    mut request: Request<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    let flash_addr = match self.platform.ota_begin().await {
      Ok(addr) => addr,
      Err(e) => {
        return format!("OTA begin error: {e:?}")
          .write_to(request.body_connection.finalize().await?, response_writer)
          .await;
      }
    };

    let mut reader = request.body_connection.body().reader();
    let mut buffer = [0u8; 4096];
    let mut current_offset = flash_addr;
    let mut total_size = 0u32;

    loop {
      let mut read_size = 0;
      loop {
        let chunk = reader.read(&mut buffer[read_size..]).await?;
        read_size += chunk;
        if chunk == 0 { break; }
      }
      if read_size == 0 { break; }

      if let Err(e) = self.platform.ota_write_chunk(current_offset, &buffer[..read_size]).await {
        return format!("OTA write error: {e:?}")
          .write_to(request.body_connection.finalize().await?, response_writer)
          .await;
      }

      current_offset += read_size as u32;
      total_size += read_size as u32;
    }

    if let Err(e) = self.platform.ota_commit().await {
      return format!("OTA commit error: {e:?}")
        .write_to(request.body_connection.finalize().await?, response_writer)
        .await;
    }

    format!("OK: {total_size} bytes").write_to(request.body_connection.finalize().await?, response_writer).await
  }
}
