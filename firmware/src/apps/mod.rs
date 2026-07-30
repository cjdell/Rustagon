// Re-export shared apps from the app crate
pub use app::apps::{
  AppAction, AppName, MenuApp, MenuAppContext, MenuAppInput, MenuAppType,
  app_store::AppStoreApp, config::ConfigApp, files::FilesApp, input_test::InputTestApp, ota_updater::OtaUpdaterApp,
  power_info::PowerInfoApp, wifi_scanner::WifiScannerApp,
};
