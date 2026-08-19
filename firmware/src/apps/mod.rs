// Re-export shared apps from the app crate
pub use app::apps::{
  AppAction, AppInput, AppName, AppRunContext, AppRunEvent, MenuApp, MenuAppContext, MenuAppType, app_store::AppStoreApp,
  config::ConfigApp, files::FilesApp, input_test::InputTestApp, ota_updater::OtaUpdaterApp, power_info::PowerInfoApp,
  wifi_scanner::WifiScannerApp,
};
