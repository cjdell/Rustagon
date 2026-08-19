pub mod common;

pub use crate::native::common::{NativeApp, NativeAppContext, NativeAppName};

use alloc::boxed::Box;
use alloc::string::String;
use core::future::Future;
use core::pin::Pin;

pub enum NativeAppType {
  // ExampleNativeAsyncApp stays in firmware for now
}

impl NativeAppType {
  pub fn list_apps() -> [&'static str; 0] {
    []
  }

  pub fn load_app_async(_name: String, _ctx: NativeAppContext) -> NativeAppType {
    panic!("No native apps available")
  }
}

impl NativeAppType {
  pub fn app_main(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    match *self {}
  }
}
