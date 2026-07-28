use crate::{
  platform::{Platform, WifiStatus},
  types::*,
  utils::led_service::LedState,
};
use alloc::string::String;
use log::info;

/// Monitor WiFi status changes and update LCD/LED accordingly.
/// Uses temporary notification overlays that automatically restore
/// whatever screen was being shown before the status change.
#[embassy_executor::task]
pub async fn wifi_monitor_task(platform: crate::platform::HardwarePlatform) {
  loop {
    let status = platform.wifi_manager().wait_for_status_change().await;
    info!("WiFi Status: {:?}", status);

    match status {
      WifiStatus::Connected(_ipv4_addr) => {
        platform.display_manager().signal(LcdScreen::Notification(Icon40::Wifi, String::from("Connected"))).ok();
        platform.led_manager().request(LedRequest::Solid(LedState { r: 0, g: 0, b: 255 })).ok();
      }
      WifiStatus::AccessPoint => {
        platform.display_manager().signal(LcdScreen::Notification(Icon40::Info, String::from("AP Mode Active"))).ok();
        platform.led_manager().request(LedRequest::Fire).ok();
      }
      WifiStatus::Offline | WifiStatus::NoNetworksFound | WifiStatus::Interrupted => {
        platform.display_manager().signal(LcdScreen::Notification(Icon40::Warn, String::from("WiFi Offline"))).ok();
        platform.led_manager().request(LedRequest::Solid(LedState { r: 255, g: 0, b: 0 })).ok();
      }
      WifiStatus::Connecting => {
        platform.display_manager().signal(LcdScreen::Notification(Icon40::Info, String::from("Connecting..."))).ok();
        platform.led_manager().request(LedRequest::Solid(LedState { r: 255, g: 165, b: 0 })).ok();
      }
    }
  }
}
