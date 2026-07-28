use app::apps::common::AppName;

/// Returns the full list of all available apps, including firmware-specific ones.
pub fn list_all_apps() -> [&'static str; 5] {
  [
    <crate::apps::ConfigApp<crate::platform::HardwarePlatform> as AppName>::app_name(),
    <crate::apps::FilesApp<crate::platform::HardwarePlatform> as AppName>::app_name(),
    <crate::apps::WifiScannerApp<crate::platform::HardwarePlatform> as AppName>::app_name(),
    "App Store",
    "Firmware Update",
  ]
}
