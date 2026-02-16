use crate::{
  lib::{DeviceConfigurator, DeviceState, KnownWifiNetwork, WifiMode},
  tasks::wifi::{WifiCommandMessage, WifiCommandSender},
};
use alloc::{format, vec::Vec};
use embedded_io::ReadExactError;
use embedded_io_async::Read;
use esp_alloc::ExternalMemory;
use picoserve::{
  ResponseSent,
  request::Request,
  response::{IntoResponse as _, ResponseWriter},
  routing::RequestHandlerService,
};

pub struct HandleWifiJoin {
  device_state: DeviceState,
  wifi_command_sender: WifiCommandSender,
}

impl HandleWifiJoin {
  pub fn new(device_state: DeviceState, wifi_command_sender: WifiCommandSender) -> Self {
    Self {
      device_state,
      wifi_command_sender,
    }
  }
}

impl RequestHandlerService<()> for HandleWifiJoin {
  async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    mut request: Request<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    let buffer = read_request_to_buffer!(request, response_writer);

    let network = match serde_json::from_slice::<KnownWifiNetwork>(&buffer) {
      Ok(network) => network,
      Err(err) => return format_response!(request, response_writer, "Error parsing JSON: {err:?}"),
    };

    if let Err(err) = self.device_state.add_known_wifi_network(network.ssid.clone(), network.pass.clone()) {
      return format_response!(request, response_writer, "Error saving wifi network: {err:?}");
    }

    match self.device_state.get_data().wifi_mode {
      WifiMode::Station => {
        self.wifi_command_sender.send(WifiCommandMessage::OverrideConnect(network.ssid, network.pass)).await;
      }
      WifiMode::AccessPoint => {
        if let Err(err) = self.device_state.set_wifi_mode(WifiMode::Station) {
          return format_response!(request, response_writer, "Error changing wifi mode: {err:?}");
        }

        // Need to restart to change mode
        esp_hal::system::software_reset();
      }
    };

    return "Done".write_to(request.body_connection.finalize().await?, response_writer).await;
  }
}
