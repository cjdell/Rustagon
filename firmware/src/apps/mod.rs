// Re-export shared apps from the app crate
pub use app::apps::{MenuAppAsync, MenuAppContext, MenuAppInput, MenuAppInputChannel, MenuAppInputReceiver, MenuAppType, common::AppName};
pub use app::apps::{config::ConfigApp, files::FilesApp, wifi_scanner::WifiScannerApp};

// Firmware-specific apps (depend on ESP32 HTTP client)
pub mod app_store;
pub mod ota_updater;
