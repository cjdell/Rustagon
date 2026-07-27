use crate::platform::ConfigHandle;
use crate::types::{DeviceConfig, KnownWifiNetwork};
use alloc::string::String;

pub trait DeviceConfigurator {
  fn get_wifi_mode(&self) -> impl Future<Output = crate::types::WifiMode>;
  fn set_wifi_mode(&self, mode: crate::types::WifiMode) -> impl Future<Output = Result<(), crate::platform::StateError>>;
  fn add_known_wifi_network(&self, ssid: String, pass: String) -> impl Future<Output = Result<(), crate::platform::StateError>>;
}

impl DeviceConfigurator for ConfigHandle {
  async fn get_wifi_mode(&self) -> crate::types::WifiMode {
    self.get_data().await.wifi_mode
  }

  async fn set_wifi_mode(&self, mode: crate::types::WifiMode) -> Result<(), crate::platform::StateError> {
    let mut data = self.get_data().await;
    data.wifi_mode = mode;
    self.set_data(data).await;
    self.save().await
  }

  async fn add_known_wifi_network(&self, ssid: String, pass: String) -> Result<(), crate::platform::StateError> {
    let mut data = self.get_data().await;
    let mut found = false;

    for known in &mut data.known_wifi_networks {
      if known.ssid == ssid {
        found = true;
        known.pass = pass.clone();
      }
    }

    if !found {
      data.known_wifi_networks.push(KnownWifiNetwork {
        ssid: ssid.clone(),
        pass: pass.clone(),
      });
    }

    data.wifi_mode = crate::types::WifiMode::Station;

    self.set_data(data).await;
    self.save().await
  }
}
