use crate::platform::StorageHandle;
use crate::protocol::{HostIpcReceiver, WasmIpcSender};

pub trait NativeApp {
  async fn app_main(&self);
}

pub trait NativeAppName {
  fn app_name() -> &'static str;
}

pub struct NativeAppContext {
  pub storage: StorageHandle,
  pub sender: WasmIpcSender,
  pub receiver: HostIpcReceiver,
}

impl NativeAppContext {
  pub fn new(storage: StorageHandle, sender: WasmIpcSender, receiver: HostIpcReceiver) -> Self {
    Self { storage, sender, receiver }
  }
}
