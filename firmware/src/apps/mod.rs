pub mod app_store;
pub mod common;
pub mod config;
pub mod files;
pub mod ota_updater;
pub mod wifi_scanner;

pub use common::{MenuAppAsync, MenuAppContext, MenuAppInput, MenuAppInputChannel, MenuAppInputReceiver};

use crate::apps::{app_store::AppStoreApp, common::AppName, config::ConfigApp, files::FilesApp, ota_updater::OtaUpdaterApp, wifi_scanner::WifiScannerApp};
use alloc::string::String;

pub enum MenuAppType {
  AppStoreApp(AppStoreApp),
  ConfigApp(ConfigApp),
  FilesApp(FilesApp),
  OtaUpdaterApp(OtaUpdaterApp),
  WifiScannerApp(WifiScannerApp),
}

impl MenuAppAsync for MenuAppType {
  async fn work(&mut self) -> bool {
    match self {
      MenuAppType::AppStoreApp(app) => app.work().await,
      MenuAppType::ConfigApp(app) => app.work().await,
      MenuAppType::FilesApp(app) => app.work().await,
      MenuAppType::OtaUpdaterApp(app) => app.work().await,
      MenuAppType::WifiScannerApp(app) => app.work().await,
    }
  }
}

impl MenuAppType {
  pub fn list_apps() -> [&'static str; 5] {
    [
      ConfigApp::app_name(),
      FilesApp::app_name(),
      WifiScannerApp::app_name(),
      AppStoreApp::app_name(),
      OtaUpdaterApp::app_name(),
    ]
  }

  pub fn load_app_async(name: String, ctx: MenuAppContext) -> MenuAppType {
    if name == ConfigApp::app_name() {
      return MenuAppType::ConfigApp(ConfigApp::new(ctx));
    }
    if name == FilesApp::app_name() {
      return MenuAppType::FilesApp(FilesApp::new(ctx));
    }
    if name == WifiScannerApp::app_name() {
      return MenuAppType::WifiScannerApp(WifiScannerApp::new(ctx));
    }
    if name == AppStoreApp::app_name() {
      return MenuAppType::AppStoreApp(AppStoreApp::new(ctx));
    }
    if name == OtaUpdaterApp::app_name() {
      return MenuAppType::OtaUpdaterApp(OtaUpdaterApp::new(ctx));
    }

    panic!("App not found!")
  }
}
