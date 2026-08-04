pub mod app_store;
pub mod common;
pub mod config;
pub mod editor;
pub mod files;
pub mod hexpansion_viewer;
pub mod input_test;
pub mod ota_updater;
pub mod power_info;
pub mod ssh;
pub mod wifi_scanner;

pub use common::{AppAction, AppEvent, AppName, MenuApp, MenuAppContext, MenuAppInput};

use crate::apps::{
  app_store::AppStoreApp, config::ConfigApp, editor::EditorApp, files::FilesApp, hexpansion_viewer::HexpansionViewerApp,
  input_test::InputTestApp, ota_updater::OtaUpdaterApp, power_info::PowerInfoApp, ssh::SshApp, wifi_scanner::WifiScannerApp,
};
use crate::platform::Platform;
pub enum MenuAppType<P: Platform> {
  AppStoreApp(AppStoreApp<P>),
  ConfigApp(ConfigApp<P>),
  EditorApp(EditorApp<P>),
  FilesApp(FilesApp<P>),
  HexpansionViewerApp(HexpansionViewerApp<P>),
  InputTestApp(InputTestApp<P>),
  OtaUpdaterApp(OtaUpdaterApp<P>),
  PowerInfoApp(PowerInfoApp<P>),
  SshApp(SshApp<P>),
  WifiScannerApp(WifiScannerApp<P>),
}

impl<P: Platform> MenuApp for MenuAppType<P> {
  async fn init(&mut self) {
    match self {
      MenuAppType::AppStoreApp(app) => app.init().await,
      MenuAppType::ConfigApp(app) => app.init().await,
      MenuAppType::EditorApp(app) => app.init().await,
      MenuAppType::FilesApp(app) => app.init().await,
      MenuAppType::HexpansionViewerApp(app) => app.init().await,
      MenuAppType::InputTestApp(app) => app.init().await,
      MenuAppType::OtaUpdaterApp(app) => app.init().await,
      MenuAppType::PowerInfoApp(app) => app.init().await,
      MenuAppType::SshApp(app) => app.init().await,
      MenuAppType::WifiScannerApp(app) => app.init().await,
    }
  }

  fn render(&self) -> display_types::LcdScreen {
    match self {
      MenuAppType::AppStoreApp(app) => app.render(),
      MenuAppType::ConfigApp(app) => app.render(),
      MenuAppType::EditorApp(app) => app.render(),
      MenuAppType::FilesApp(app) => app.render(),
      MenuAppType::HexpansionViewerApp(app) => app.render(),
      MenuAppType::InputTestApp(app) => app.render(),
      MenuAppType::OtaUpdaterApp(app) => app.render(),
      MenuAppType::PowerInfoApp(app) => app.render(),
      MenuAppType::SshApp(app) => app.render(),
      MenuAppType::WifiScannerApp(app) => app.render(),
    }
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    match self {
      MenuAppType::AppStoreApp(app) => app.handle_input(input).await,
      MenuAppType::ConfigApp(app) => app.handle_input(input).await,
      MenuAppType::EditorApp(app) => app.handle_input(input).await,
      MenuAppType::FilesApp(app) => app.handle_input(input).await,
      MenuAppType::HexpansionViewerApp(app) => app.handle_input(input).await,
      MenuAppType::InputTestApp(app) => app.handle_input(input).await,
      MenuAppType::OtaUpdaterApp(app) => app.handle_input(input).await,
      MenuAppType::PowerInfoApp(app) => app.handle_input(input).await,
      MenuAppType::SshApp(app) => app.handle_input(input).await,
      MenuAppType::WifiScannerApp(app) => app.handle_input(input).await,
    }
  }

  async fn handle_event(&mut self, event: AppEvent) {
    match self {
      MenuAppType::AppStoreApp(app) => app.handle_event(event).await,
      MenuAppType::ConfigApp(app) => app.handle_event(event).await,
      MenuAppType::EditorApp(app) => app.handle_event(event).await,
      MenuAppType::FilesApp(app) => app.handle_event(event).await,
      MenuAppType::HexpansionViewerApp(app) => app.handle_event(event).await,
      MenuAppType::InputTestApp(app) => app.handle_event(event).await,
      MenuAppType::OtaUpdaterApp(app) => app.handle_event(event).await,
      MenuAppType::PowerInfoApp(app) => app.handle_event(event).await,
      MenuAppType::SshApp(app) => app.handle_event(event).await,
      MenuAppType::WifiScannerApp(app) => app.handle_event(event).await,
    }
  }
}

impl<P: Platform> MenuAppType<P> {
  pub fn list_apps() -> [&'static str; 10] {
    [
      AppStoreApp::<P>::app_name(),
      ConfigApp::<P>::app_name(),
      EditorApp::<P>::app_name(),
      FilesApp::<P>::app_name(),
      HexpansionViewerApp::<P>::app_name(),
      InputTestApp::<P>::app_name(),
      OtaUpdaterApp::<P>::app_name(),
      PowerInfoApp::<P>::app_name(),
      SshApp::<P>::app_name(),
      WifiScannerApp::<P>::app_name(),
    ]
  }

  pub fn load_app_async(name: &str, ctx: MenuAppContext<P>) -> Result<MenuAppType<P>, MenuAppContext<P>> {
    if name == AppStoreApp::<P>::app_name() {
      return Ok(MenuAppType::AppStoreApp(AppStoreApp::new(ctx)));
    }
    if name == ConfigApp::<P>::app_name() {
      return Ok(MenuAppType::ConfigApp(ConfigApp::new(ctx)));
    }
    if name == EditorApp::<P>::app_name() {
      return Ok(MenuAppType::EditorApp(EditorApp::new(ctx)));
    }
    if name == FilesApp::<P>::app_name() {
      return Ok(MenuAppType::FilesApp(FilesApp::new(ctx)));
    }
    if name == HexpansionViewerApp::<P>::app_name() {
      return Ok(MenuAppType::HexpansionViewerApp(HexpansionViewerApp::new(ctx)));
    }
    if name == InputTestApp::<P>::app_name() {
      return Ok(MenuAppType::InputTestApp(InputTestApp::new(ctx)));
    }
    if name == OtaUpdaterApp::<P>::app_name() {
      return Ok(MenuAppType::OtaUpdaterApp(OtaUpdaterApp::new(ctx)));
    }
    if name == PowerInfoApp::<P>::app_name() {
      return Ok(MenuAppType::PowerInfoApp(PowerInfoApp::new(ctx)));
    }
    if name == SshApp::<P>::app_name() {
      return Ok(MenuAppType::SshApp(SshApp::new(ctx)));
    }
    if name == WifiScannerApp::<P>::app_name() {
      return Ok(MenuAppType::WifiScannerApp(WifiScannerApp::new(ctx)));
    }
    Err(ctx)
  }
}
