pub mod types;

pub use types::*;

use crate::platform::{HardwarePlatform, Platform};
use log::info;

#[embassy_executor::task]
pub async fn menu_task(runner_ctx: crate::tasks::menu::types::MenuRunnerContext) {
  info!("Starting Menu Task...");

  runner_ctx.platform.wifi_manager().set_desired_state(crate::platform::WifiDesiredState::Online).await;

  let app_ctx = app::menu::types::MenuRunnerContext {
    platform: runner_ctx.platform,
    host_ipc_sender: runner_ctx.host_ipc_sender,
    stack_event_handle: runner_ctx.stack_event_handle,
    app_loader: None,
    additional_apps: &[],
  };

  app::menu::menu_task::<HardwarePlatform>(app_ctx).await;
}
