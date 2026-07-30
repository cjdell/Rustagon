pub mod app_store;
pub mod common;
pub mod config;
pub mod files;
pub mod input_test;
pub mod ota_updater;
pub mod power_info;
pub mod wifi_scanner;

pub use common::{AppName, MenuAppAsync, MenuAppContext, MenuAppInput, MenuAppInputChannel, MenuAppInputReceiver};

use crate::platform::Platform;
use crate::apps::{
  app_store::AppStoreApp, common::AppName as _, config::ConfigApp, files::FilesApp, input_test::InputTestApp,
  ota_updater::OtaUpdaterApp, power_info::PowerInfoApp, wifi_scanner::WifiScannerApp,
};
use alloc::string::String;

pub enum MenuAppType<P: Platform> {
  ConfigApp(ConfigApp<P>),
  FilesApp(FilesApp<P>),
  InputTestApp(InputTestApp<P>),
  PowerInfoApp(PowerInfoApp<P>),
  WifiScannerApp(WifiScannerApp<P>),
  AppStoreApp(AppStoreApp<P>),
  OtaUpdaterApp(OtaUpdaterApp<P>),
}

impl<P: Platform> MenuAppAsync for MenuAppType<P> {
  async fn work(&mut self) -> bool {
    match self {
      MenuAppType::ConfigApp(app) => app.work().await,
      MenuAppType::FilesApp(app) => app.work().await,
      MenuAppType::InputTestApp(app) => app.work().await,
      MenuAppType::PowerInfoApp(app) => app.work().await,
      MenuAppType::WifiScannerApp(app) => app.work().await,
      MenuAppType::AppStoreApp(app) => app.work().await,
      MenuAppType::OtaUpdaterApp(app) => app.work().await,
    }
  }
}

impl<P: Platform> MenuAppType<P> {
  pub fn list_apps() -> [&'static str; 7] {
    [
      ConfigApp::<P>::app_name(),
      FilesApp::<P>::app_name(),
      InputTestApp::<P>::app_name(),
      PowerInfoApp::<P>::app_name(),
      WifiScannerApp::<P>::app_name(),
      AppStoreApp::<P>::app_name(),
      OtaUpdaterApp::<P>::app_name(),
    ]
  }

  pub fn load_app_async(name: &str, ctx: MenuAppContext<P>) -> Result<MenuAppType<P>, MenuAppContext<P>> {
    if name == ConfigApp::<P>::app_name() {
      return Ok(MenuAppType::ConfigApp(ConfigApp::new(ctx)));
    }
    if name == FilesApp::<P>::app_name() {
      return Ok(MenuAppType::FilesApp(FilesApp::new(ctx)));
    }
    if name == InputTestApp::<P>::app_name() {
      return Ok(MenuAppType::InputTestApp(InputTestApp::new(ctx)));
    }
    if name == PowerInfoApp::<P>::app_name() {
      return Ok(MenuAppType::PowerInfoApp(PowerInfoApp::new(ctx)));
    }
    if name == WifiScannerApp::<P>::app_name() {
      return Ok(MenuAppType::WifiScannerApp(WifiScannerApp::new(ctx)));
    }
    if name == AppStoreApp::<P>::app_name() {
      return Ok(MenuAppType::AppStoreApp(AppStoreApp::new(ctx)));
    }
    if name == OtaUpdaterApp::<P>::app_name() {
      return Ok(MenuAppType::OtaUpdaterApp(OtaUpdaterApp::new(ctx)));
    }
    Err(ctx)
  }
}
