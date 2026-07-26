use crate::{
  platform::{Platform, WifiStatus},
  types::*,
  utils::*,
};
use alloc::format;
use alloc::string::String;
use embassy_time::{Duration, Timer};
use esp_println::println;

/// Monitor WiFi status changes and update LCD/LED accordingly
#[embassy_executor::task]
pub async fn wifi_monitor_task(platform: crate::platform::HardwarePlatform, device_state: DeviceState) {
  loop {
    let status = platform.wifi_manager().wait_for_status_change().await;
    println!("WiFi Status: {:?}", status);

    match status {
      WifiStatus::Connected(ipv4_addr) => {
        let _ = platform.display_manager().signal(LcdScreen::Headline(Icon40::Wifi, String::from("Connected")));
        let _ = platform.led_manager().request(LedRequest::Solid(LedState { r: 0, g: 0, b: 255 }));
        Timer::after(Duration::from_millis(2_000)).await;
        let _ = platform.led_manager().request(LedRequest::Rainbow);
        let _ = platform.display_manager().signal(LcdScreen::Headline(Icon40::Info, format!("IP: {}", ipv4_addr)));
        Timer::after(Duration::from_millis(2_000)).await;
      }
      WifiStatus::AccessPoint => {
        let _ = platform.display_manager().signal(LcdScreen::Headline(Icon40::Info, String::from("AP Mode Active")));
        let _ = platform.led_manager().request(LedRequest::Fire);
        Timer::after(Duration::from_millis(2_000)).await;
        let _ = platform.display_manager().signal(LcdScreen::Headline(
          Icon40::Info,
          format!("AP: {}", device_state.get_data().ap_ssid),
        ));
        Timer::after(Duration::from_millis(2_000)).await;
        let _ = platform.display_manager().signal(LcdScreen::Headline(Icon40::Info, String::from("IP: 192.168.1.1")));
        Timer::after(Duration::from_millis(2_000)).await;
      }
      WifiStatus::Offline | WifiStatus::NoNetworksFound | WifiStatus::Interrupted => {
        let _ = platform.display_manager().signal(LcdScreen::Headline(Icon40::Warn, String::from("WiFi Offline")));
        let _ = platform.led_manager().request(LedRequest::Solid(LedState { r: 255, g: 0, b: 0 }));
        Timer::after(Duration::from_millis(1_000)).await;
      }
      WifiStatus::Connecting => {
        let _ = platform.display_manager().signal(LcdScreen::Headline(Icon40::Info, String::from("Connecting...")));
        let _ = platform.led_manager().request(LedRequest::Solid(LedState { r: 255, g: 165, b: 0 }));
        Timer::after(Duration::from_millis(1_000)).await;
      }
    }
  }
}
