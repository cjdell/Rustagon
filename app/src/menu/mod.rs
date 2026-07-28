pub mod execute;
pub mod menus;
pub mod state;
pub mod types;

pub use types::*;

use crate::{
  apps::*,
  menu::state::*,
  menu::types::{MenuContext, MenuRunnerContext},
  platform::Platform,
  protocol::HostIpcMessage,
  types::*,
};
use alloc::string::ToString as _;
use alloc::sync::Arc;
use alloc::vec::Vec;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, rwlock::RwLock};
use log::info;
use menus::MenuProvider as _;

pub async fn menu_task<P: Platform>(mut runner_ctx: MenuRunnerContext<P>) {
  let menu_app_input_channel = crate::apps::common::create_menu_app_channel();

  let ctx = MenuContext {
    stack: runner_ctx.stack,
    storage: runner_ctx.storage.clone(),
    platform: runner_ctx.platform.clone(),
    host_ipc_sender: runner_ctx.host_ipc_sender,
    display: runner_ctx.platform.display_manager(),
    menu_app_input_channel,
  };

  let app = Arc::new(RwLock::new(AppState::None));

  let mut state = MenuState {
    ctx: ctx.clone(),
    app: app.clone(),
    current_menu: Menu::Root,
    menu_options: Vec::new(),
    selected: 0,
    http_message: HttpStatusMessage::Idle,
  };

  state.menu_options = state.get_menu_provider().await.get_items().await;

  loop {
    state.refresh().await;

    let input = embassy_futures::select::select(
      runner_ctx.platform.system_manager().next_button(),
      runner_ctx.platform.input_manager().next_button(),
    ).await;

    match input {
      embassy_futures::select::Either::First(system) => {
        match system {
          SystemMessage::BootButton => {
            if let Ok(app) = state.app.try_read() {
              match *app {
                AppState::MenuApp => { menu_app_input_channel.send(MenuAppInput::Stop).await; }
                AppState::HostedApp => { runner_ctx.host_ipc_sender.send((0, HostIpcMessage::Stop)).await; }
                _ => {}
              }
            }
          }
        }
      }
      embassy_futures::select::Either::Second(hex) => {
        let app_running = app.try_read()
          .map(|app| !matches!(*app, AppState::None))
          .unwrap_or(true);

        if app_running {
          match *app.write().await {
            AppState::MenuApp => {
              menu_app_input_channel.send(MenuAppInput::HexButton(hex)).await;
              continue;
            }
            AppState::HostedApp => {
              runner_ctx.host_ipc_sender.send((0, HostIpcMessage::HexButton(hex))).await;
              continue;
            }
            _ => {}
          }
        }

        match hex {
          HexButton::Up => { if state.selected > 0 { state.selected -= 1; } }
          HexButton::Right => { runner_ctx.platform.led_manager().request(LedRequest::Sparkle(LedState::new(255, 255, 255))); }
          HexButton::Fire => { state.execute_option().await; }
          HexButton::Down => { state.selected += 1; }
          _ => {}
        }
      }
    }
  }
}
