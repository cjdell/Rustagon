use super::common::*;
use crate::platform::ConfigHandle;
use crate::types::DeviceConfig;
use alloc::{format, vec::Vec};
use picoserve::{
  ResponseSent, io::Read,
  request::Request,
  response::{IntoResponse, ResponseWriter},
  routing::RequestHandlerService,
};

pub struct GetConfigHandler {
  config: ConfigHandle<DeviceConfig>,
}

impl GetConfigHandler {
  pub fn new(config: ConfigHandle<DeviceConfig>) -> Self {
    Self { config }
  }
}

impl RequestHandlerService<()> for GetConfigHandler {
  async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    request: Request<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    let json = match self.config.get_json().await {
      Ok(json) => json,
      Err(err) => return format_response!(request, response_writer, "Error reading JSON: {err:?}"),
    };
    json_response!(request, response_writer, json.as_str())
  }
}

pub struct SaveConfigHandler {
  config: ConfigHandle<DeviceConfig>,
}

impl SaveConfigHandler {
  pub fn new(config: ConfigHandle<DeviceConfig>) -> Self {
    Self { config }
  }
}

impl RequestHandlerService<()> for SaveConfigHandler {
  async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    mut request: Request<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    let buffer = read_request_to_buffer!(request, response_writer);

    if let Err(err) = self.config.set_json(buffer.to_vec()).await {
      return format_response!(request, response_writer, "Error applying JSON: {err:?}");
    }
    if let Err(err) = self.config.save().await {
      return format_response!(request, response_writer, "Error save JSON: {err:?}");
    }

    "Done".write_to(request.body_connection.finalize().await?, response_writer).await
  }
}
