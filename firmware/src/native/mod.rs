mod common;
mod example_async;

pub use crate::native::common::{NativeApp, NativeAppContext};

use crate::native::{common::NativeAppName as _, example_async::ExampleNativeAsyncApp};
use alloc::string::String;

pub enum NativeAppType {
  ExampleNativeAsyncApp(ExampleNativeAsyncApp),
}

impl NativeApp for NativeAppType {
  async fn app_main(&self) -> () {
    match self {
      NativeAppType::ExampleNativeAsyncApp(app) => app.app_main().await,
    }
  }
}

impl NativeAppType {
  pub fn list_apps() -> [&'static str; 1] {
    [ExampleNativeAsyncApp::app_name()]
  }

  pub fn load_app_async(name: String, ctx: NativeAppContext) -> NativeAppType {
    if name == ExampleNativeAsyncApp::app_name() {
      return NativeAppType::ExampleNativeAsyncApp(ExampleNativeAsyncApp::new(ctx));
    }

    panic!("App not found!")
  }
}
