pub mod app_store;
pub mod common;
pub mod ota_updater;

use crate::apps::{app_store::AppStoreApp, common::AppName, ota_updater::OtaUpdaterApp};
use alloc::string::String;

pub use common::{MenuAppAsync, MenuAppContext, MenuAppInput, MenuAppInputChannel, MenuAppInputReceiver};

pub enum MenuAppType {
  AppStoreApp(AppStoreApp),
  OtaUpdaterApp(OtaUpdaterApp),
}

impl MenuAppAsync for MenuAppType {
  async fn work(&mut self) -> bool {
    match self {
      MenuAppType::AppStoreApp(app) => app.work().await,
      MenuAppType::OtaUpdaterApp(app) => app.work().await,
    }
  }
}

impl MenuAppType {
  pub fn list_apps() -> [&'static str; 2] {
    [AppStoreApp::app_name(), OtaUpdaterApp::app_name()]
  }

  pub fn load_app_async(name: String, ctx: MenuAppContext) -> MenuAppType {
    if name == AppStoreApp::app_name() {
      return MenuAppType::AppStoreApp(AppStoreApp::new(ctx));
    }
    if name == OtaUpdaterApp::app_name() {
      return MenuAppType::OtaUpdaterApp(OtaUpdaterApp::new(ctx));
    }

    panic!("App not found!")
  }
}
