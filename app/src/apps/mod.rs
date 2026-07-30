pub mod app_store;
pub mod common;
pub mod config;
pub mod files;
pub mod input_test;
pub mod ota_updater;
pub mod power_info;
pub mod wifi_scanner;

pub use common::{AppAction, AppName, MenuApp, MenuAppContext, MenuAppInput};

use crate::platform::Platform;
use crate::apps::{
  app_store::AppStoreApp, config::ConfigApp, files::FilesApp, input_test::InputTestApp,
  ota_updater::OtaUpdaterApp, power_info::PowerInfoApp, wifi_scanner::WifiScannerApp,
};
pub enum MenuAppType<P: Platform> {
  ConfigApp(ConfigApp<P>),
  FilesApp(FilesApp<P>),
  InputTestApp(InputTestApp<P>),
  PowerInfoApp(PowerInfoApp<P>),
  WifiScannerApp(WifiScannerApp<P>),
  AppStoreApp(AppStoreApp<P>),
  OtaUpdaterApp(OtaUpdaterApp<P>),
}

impl<P: Platform> MenuApp for MenuAppType<P> {
  async fn init(&mut self) {
    match self {
      MenuAppType::ConfigApp(app) => app.init().await,
      MenuAppType::FilesApp(app) => app.init().await,
      MenuAppType::InputTestApp(app) => app.init().await,
      MenuAppType::PowerInfoApp(app) => app.init().await,
      MenuAppType::WifiScannerApp(app) => app.init().await,
      MenuAppType::AppStoreApp(app) => app.init().await,
      MenuAppType::OtaUpdaterApp(app) => app.init().await,
    }
  }

  fn render(&self) -> display_types::LcdScreen {
    match self {
      MenuAppType::ConfigApp(app) => app.render(),
      MenuAppType::FilesApp(app) => app.render(),
      MenuAppType::InputTestApp(app) => app.render(),
      MenuAppType::PowerInfoApp(app) => app.render(),
      MenuAppType::WifiScannerApp(app) => app.render(),
      MenuAppType::AppStoreApp(app) => app.render(),
      MenuAppType::OtaUpdaterApp(app) => app.render(),
    }
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    match self {
      MenuAppType::ConfigApp(app) => app.handle_input(input).await,
      MenuAppType::FilesApp(app) => app.handle_input(input).await,
      MenuAppType::InputTestApp(app) => app.handle_input(input).await,
      MenuAppType::PowerInfoApp(app) => app.handle_input(input).await,
      MenuAppType::WifiScannerApp(app) => app.handle_input(input).await,
      MenuAppType::AppStoreApp(app) => app.handle_input(input).await,
      MenuAppType::OtaUpdaterApp(app) => app.handle_input(input).await,
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
