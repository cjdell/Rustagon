use crate::tasks::{
  http::common::json_response,
  wifi::{ScanWatch, WifiCommandMessage, WifiCommandSender},
};
use alloc::vec;
use embedded_io_async::Read;
use picoserve::{
  ResponseSent,
  request::Request,
  response::{IntoResponse as _, ResponseWriter},
  routing::RequestHandlerService,
};

pub struct HandleWifiScan {
  wifi_command_sender: WifiCommandSender,
  scan_signal: &'static ScanWatch,
}

impl HandleWifiScan {
  pub fn new(wifi_command_sender: WifiCommandSender, scan_signal: &'static ScanWatch) -> Self {
    Self {
      wifi_command_sender,
      scan_signal,
    }
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
    self.wifi_command_sender.send(WifiCommandMessage::Scan).await;

    let mut scan_receiver = self.scan_signal.receiver().unwrap();

    let results = match timeout_result!(scan_receiver.get(), 5_000, "Scan") {
      Ok(results) => results,
      Err(_) => vec![],
    };

    let json = serde_json::to_string(&results).unwrap();

    json_response!(request, response_writer, &json)
  }
}
