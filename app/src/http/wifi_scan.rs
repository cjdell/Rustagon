use super::common::*;
use crate::platform::Platform;
use alloc::vec::Vec;
use picoserve::{
  ResponseSent, io::Read,
  request::Request,
  response::IntoResponse,
  routing::RequestHandlerService,
};

pub struct HandleWifiScan<P: Platform> {
  platform: P,
}

impl<P: Platform> HandleWifiScan<P> {
  pub fn new(platform: P) -> Self {
    Self { platform }
  }
}

impl<P: Platform> RequestHandlerService<()> for HandleWifiScan<P> {
  async fn call_request_handler_service<R: Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    request: Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let results = self.platform.wifi_manager().scan().await.unwrap_or_default();
    let json = serde_json::to_string(&results).unwrap();
    json_response!(request, response_writer, &json)
  }
}
