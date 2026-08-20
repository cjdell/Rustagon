pub mod app_store;
pub mod common;
pub mod config;
pub mod confirm;
pub mod editor;
pub mod files;
pub mod hexpansion_viewer;
pub mod input_test;
pub mod ota_updater;
pub mod power_info;
pub mod ssh;
pub mod wifi_scanner;

pub use common::{
  AppAction, AppError, AppEvent, AppInput, AppName, AppParams, AppResult, AppRunContext, AppRunEvent, MenuApp, MenuAppContext,
  ResultChannel,
};

pub use common::nav_button_from_device_event;

use crate::apps::{
  app_store::AppStoreApp, config::ConfigApp, confirm::ConfirmationApp, editor::EditorApp, files::FilesApp,
  hexpansion_viewer::HexpansionViewerApp, input_test::InputTestApp, ota_updater::OtaUpdaterApp, power_info::PowerInfoApp, ssh::SshApp,
  wifi_scanner::WifiScannerApp,
};
use crate::platform::Platform;
pub enum MenuAppType<P: Platform> {
  AppStoreApp(AppStoreApp<P>),
  ConfirmationApp(ConfirmationApp<P>),
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

impl<P: Platform> MenuApp<P> for MenuAppType<P> {
  fn render(&self) -> display_types::LcdScreen {
    match self {
      MenuAppType::AppStoreApp(app) => app.render(),
      MenuAppType::ConfirmationApp(app) => app.render(),
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

  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction {
    match self {
      MenuAppType::AppStoreApp(app) => app.run(ctx).await,
      MenuAppType::ConfirmationApp(app) => app.run(ctx).await,
      MenuAppType::ConfigApp(app) => app.run(ctx).await,
      MenuAppType::EditorApp(app) => app.run(ctx).await,
      MenuAppType::FilesApp(app) => app.run(ctx).await,
      MenuAppType::HexpansionViewerApp(app) => app.run(ctx).await,
      MenuAppType::InputTestApp(app) => app.run(ctx).await,
      MenuAppType::OtaUpdaterApp(app) => app.run(ctx).await,
      MenuAppType::PowerInfoApp(app) => app.run(ctx).await,
      MenuAppType::SshApp(app) => app.run(ctx).await,
      MenuAppType::WifiScannerApp(app) => app.run(ctx).await,
    }
  }

  async fn on_stop(&mut self) {
    match self {
      MenuAppType::AppStoreApp(app) => app.on_stop().await,
      MenuAppType::ConfirmationApp(app) => app.on_stop().await,
      MenuAppType::ConfigApp(app) => app.on_stop().await,
      MenuAppType::EditorApp(app) => app.on_stop().await,
      MenuAppType::FilesApp(app) => app.on_stop().await,
      MenuAppType::HexpansionViewerApp(app) => app.on_stop().await,
      MenuAppType::InputTestApp(app) => app.on_stop().await,
      MenuAppType::OtaUpdaterApp(app) => app.on_stop().await,
      MenuAppType::PowerInfoApp(app) => app.on_stop().await,
      MenuAppType::SshApp(app) => app.on_stop().await,
      MenuAppType::WifiScannerApp(app) => app.on_stop().await,
    }
  }
}

impl<P: Platform> MenuAppType<P> {
  pub fn list_apps() -> [&'static str; 11] {
    [
      AppStoreApp::<P>::app_name(),
      ConfirmationApp::<P>::app_name(),
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

  /// Load a built-in app by name. `params` is the (small, typed) payload
  /// from an [`AppAction::Push`]; apps that don't take parameters ignore it.
  pub fn load_app_async(name: &str, ctx: MenuAppContext<P>, params: AppParams) -> Result<MenuAppType<P>, MenuAppContext<P>> {
    if name == AppStoreApp::<P>::app_name() {
      return Ok(MenuAppType::AppStoreApp(AppStoreApp::new(ctx)));
    }
    if name == ConfirmationApp::<P>::app_name() {
      return Ok(MenuAppType::ConfirmationApp(ConfirmationApp::with_params(ctx, params)));
    }
    if name == ConfigApp::<P>::app_name() {
      return Ok(MenuAppType::ConfigApp(ConfigApp::new(ctx)));
    }
    if name == EditorApp::<P>::app_name() {
      return Ok(MenuAppType::EditorApp(EditorApp::new(ctx)));
    }
    if name == FilesApp::<P>::app_name() {
      return Ok(MenuAppType::FilesApp(FilesApp::with_params(ctx, params)));
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
