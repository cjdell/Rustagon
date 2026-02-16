use crate::{lib::DeviceState, tasks::http::common::json_response};
use alloc::{format, vec::Vec};
use esp_alloc::ExternalMemory;
use picoserve::{
  ResponseSent,
  io::Read,
  request::Request,
  response::{IntoResponse, ResponseWriter},
  routing::RequestHandlerService,
};

pub struct GetConfigHandler {
  device_state: DeviceState,
}

impl GetConfigHandler {
  pub fn new(device_state: DeviceState) -> Self {
    Self { device_state }
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
    let json = match self.device_state.get_json() {
      Ok(json) => json,
      Err(err) => {
        return format_response!(request, response_writer, "Error reading JSON: {err:?}");
      }
    };

    json_response!(request, response_writer, &json)
  }
}

pub struct SaveConfigHandler {
  device_state: DeviceState,
}

impl SaveConfigHandler {
  pub fn new(device_state: DeviceState) -> Self {
    Self { device_state }
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

    if let Err(err) = self.device_state.set_json(&buffer) {
      return format_response!(request, response_writer, "Error applying JSON: {err:?}");
    }

    if let Err(err) = self.device_state.save() {
      return format_response!(request, response_writer, "Error save JSON: {err:?}");
    }

    return "Done".write_to(request.body_connection.finalize().await?, response_writer).await;
  }
}
