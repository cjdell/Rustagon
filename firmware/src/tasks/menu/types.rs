use crate::platform::{HardwarePlatform, StorageHandle};
use crate::types::*;
use app::menu::state::AppState;
use alloc::sync::Arc;
use embassy_net::Stack;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};

pub struct MenuRunnerContext {
  pub stack: Stack<'static>,
  pub storage: StorageHandle,
  pub host_ipc_sender: HostIpcSender,
  pub platform: HardwarePlatform,
  pub app_state: Arc<RwLock<CriticalSectionRawMutex, AppState>>,
}
