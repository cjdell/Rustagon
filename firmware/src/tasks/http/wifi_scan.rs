use crate::{platform::Platform, timeout_result, types::*};
use alloc::vec::Vec;
use embedded_io_async::Read;
use picoserve::{
  ResponseSent,
  request::Request,
  response::{IntoResponse as _, ResponseWriter},
  routing::RequestHandlerService,
};

pub struct HandleWifiScan {
  platform: crate::platform::HardwarePlatform,
}

impl HandleWifiScan {
  pub fn new(platform: crate::platform::HardwarePlatform) -> Self {
    Self { platform }
  }
}

impl RequestHandlerService<()> for HandleWifiScan {
  async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    request: Request<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    let results = self.platform.wifi_manager().scan().await.unwrap_or_default();
    let json = serde_json::to_string(&results).unwrap();

    let json = serde_json::to_string(&results).unwrap();

    json_response!(request, response_writer, &json)
  }
}
