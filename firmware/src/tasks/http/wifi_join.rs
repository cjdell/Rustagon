use crate::{device::DeviceConfigurator as _, platform::{ConfigHandle, Platform}, types::*};
use alloc::{format, vec::Vec};
use embedded_io_async::Read;
use esp_alloc::ExternalMemory;
use picoserve::{
  ResponseSent,
  request::Request,
  response::{IntoResponse as _, ResponseWriter},
  routing::RequestHandlerService,
};

pub struct HandleWifiJoin {
  config: ConfigHandle,
  platform: crate::platform::HardwarePlatform,
}

impl HandleWifiJoin {
  pub fn new(config: ConfigHandle, platform: crate::platform::HardwarePlatform) -> Self {
    Self {
      config,
      platform,
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

    if let Err(err) = self.config.add_known_wifi_network(network.ssid.clone(), network.pass.clone()).await {
      return format_response!(request, response_writer, "Error saving wifi network: {err:?}");
    }

    match self.config.get_data().await.wifi_mode {
      WifiMode::Station => {
        // WiFi manager will automatically rescan and reconnect
        self.platform.wifi_manager().set_desired_state(crate::platform::WifiDesiredState::Online).await;
      }
      WifiMode::AccessPoint => {
        if let Err(err) = self.config.set_wifi_mode(WifiMode::Station).await {
          return format_response!(request, response_writer, "Error changing wifi mode: {err:?}");
        }

        // Need to restart to change mode
        esp_hal::system::software_reset();
      }
    };

    return "Done".write_to(request.body_connection.finalize().await?, response_writer).await;
  }
}
