use crate::platform::{HardwarePlatform, StorageHandle};
use crate::types::*;
use alloc::sync::Arc;
use embassy_net::Stack;

pub struct MenuRunnerContext {
  pub stack: Stack<'static>,
  pub storage: StorageHandle,
  pub host_ipc_sender: HostIpcSender,
  pub platform: HardwarePlatform,
  pub stack_event_handle: app::menu::state::StackEventHandle,
}
