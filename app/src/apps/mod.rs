pub mod common;
pub mod config;
pub mod files;
pub mod wifi_scanner;

pub use common::{AppName, MenuAppAsync, MenuAppContext, MenuAppInput, MenuAppInputChannel, MenuAppInputReceiver};

use crate::platform::Platform;
use crate::apps::{common::AppName as _, config::ConfigApp, files::FilesApp, wifi_scanner::WifiScannerApp};
use alloc::string::String;

pub enum MenuAppType<P: Platform> {
  ConfigApp(ConfigApp<P>),
  FilesApp(FilesApp<P>),
  WifiScannerApp(WifiScannerApp<P>),
}

impl<P: Platform> MenuAppAsync for MenuAppType<P> {
  async fn work(&mut self) -> bool {
    match self {
      MenuAppType::ConfigApp(app) => app.work().await,
      MenuAppType::FilesApp(app) => app.work().await,
      MenuAppType::WifiScannerApp(app) => app.work().await,
    }
  }
}

impl<P: Platform> MenuAppType<P> {
  pub fn list_apps() -> [&'static str; 3] {
    [
      ConfigApp::<P>::app_name(),
      FilesApp::<P>::app_name(),
      WifiScannerApp::<P>::app_name(),
    ]
  }

  pub fn load_app_async(name: String, ctx: MenuAppContext<P>) -> MenuAppType<P> {
    if name == ConfigApp::<P>::app_name() {
      return MenuAppType::ConfigApp(ConfigApp::new(ctx));
    }
    if name == FilesApp::<P>::app_name() {
      return MenuAppType::FilesApp(FilesApp::new(ctx));
    }
    if name == WifiScannerApp::<P>::app_name() {
      return MenuAppType::WifiScannerApp(WifiScannerApp::new(ctx));
    }
    panic!("App not found!")
  }
}
