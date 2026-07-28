use super::common::*;
use crate::platform::Platform;
use crate::platform::ConfigHandle;
use crate::types::{DeviceConfig, KnownWifiNetwork, WifiDesiredState, WifiMode};
use alloc::{format, string::ToString, vec::Vec};
use picoserve::{
  ResponseSent, io::Read,
  request::Request,
  response::IntoResponse,
  routing::RequestHandlerService,
};

pub struct HandleWifiJoin<P: Platform> {
  config: ConfigHandle<DeviceConfig>,
  platform: P,
}

impl<P: Platform> HandleWifiJoin<P> {
  pub fn new(config: ConfigHandle<DeviceConfig>, platform: P) -> Self {
    Self { config, platform }
  }
}

impl<P: Platform> RequestHandlerService<()> for HandleWifiJoin<P> {
  async fn call_request_handler_service<R: Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    mut request: Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let buffer = read_request_to_buffer!(request, response_writer);

    let network = match serde_json::from_slice::<KnownWifiNetwork>(&buffer) {
      Ok(network) => network,
      Err(err) => return format_response!(request, response_writer, "Error parsing JSON: {err:?}"),
    };

    {
      let mut config = self.config.get_data().await;
      config.known_wifi_networks.push(KnownWifiNetwork {
        ssid: network.ssid.clone(),
        pass: network.pass.clone(),
      });
      self.config.set_data(config).await;
      if let Err(err) = self.config.save().await {
        return format_response!(request, response_writer, "Error saving wifi network: {err:?}");
      }
    }

    match self.config.get_data().await.wifi_mode {
      WifiMode::Station => {
        self.platform.wifi_manager().set_desired_state(WifiDesiredState::Online).await;
      }
      WifiMode::AccessPoint => {
        let mut config = self.config.get_data().await;
        config.wifi_mode = WifiMode::Station;
        self.config.set_data(config).await;
        if let Err(err) = self.config.save().await {
          return format_response!(request, response_writer, "Error changing wifi mode: {err:?}");
        }
        self.platform.software_reset().await;
      }
    };

    "Done".write_to(request.body_connection.finalize().await?, response_writer).await
  }
}
