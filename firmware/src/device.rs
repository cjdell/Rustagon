use super::types::{DeviceState, KnownWifiNetwork, WifiMode};
use crate::utils::state::StateError;
use alloc::string::String;

pub trait DeviceConfigurator {
  fn get_wifi_mode(&self) -> WifiMode;
  fn set_wifi_mode(&self, mode: WifiMode) -> Result<(), StateError>;

  fn add_known_wifi_network(&self, ssid: String, pass: String) -> Result<(), StateError>;
}

impl DeviceConfigurator for DeviceState {
  fn get_wifi_mode(&self) -> WifiMode {
    let data = self.get_data();
    data.wifi_mode
  }

  fn set_wifi_mode(&self, mode: WifiMode) -> Result<(), StateError> {
    let mut data = self.get_data();
    data.wifi_mode = mode;
    self.set_data(data);
    self.save()?;
    Ok(())
  }

  fn add_known_wifi_network(&self, ssid: String, pass: String) -> Result<(), StateError> {
    let mut data = self.get_data();
    let mut found = false;

    for known_wifi_network in &mut data.known_wifi_networks {
      if known_wifi_network.ssid == ssid {
        found = true;
        known_wifi_network.pass = pass.clone();
      }
    }

    if !found {
      data.known_wifi_networks.push(KnownWifiNetwork {
        ssid: ssid.clone(),
        pass: pass.clone(),
      });
    }

    data.wifi_mode = WifiMode::Station;

    self.set_data(data);

    self.save()?;

    Ok(())
  }
}
