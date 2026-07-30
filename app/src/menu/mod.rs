pub mod execute;
pub mod menus;
pub mod state;
pub mod types;

pub use types::*;

use crate::{
  apps::common::{WASM_LAUNCHING, create_menu_app_channel},
  apps::*,
  menu::state::*,
  menu::types::{MenuContext, MenuRunnerContext},
  platform::Platform,
  types::*,
};
use alloc::{sync::Arc, vec::Vec};
use core::sync::atomic::Ordering;
use embassy_futures::join;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use log::info;
use menus::MenuProvider as _;

pub async fn menu_task<P: Platform + 'static>(runner_ctx: MenuRunnerContext<P>) {
  let menu_app_input_channel = create_menu_app_channel();

  let ctx = MenuContext {
    storage: runner_ctx.storage.clone(),
    platform: runner_ctx.platform.clone(),
    host_ipc_sender: runner_ctx.host_ipc_sender.clone(),
    display: runner_ctx.platform.display_manager(),
    menu_app_input_channel,
    additional_apps: runner_ctx.additional_apps,
  };

  let app: Arc<RwLock<CriticalSectionRawMutex, AppState>> = match runner_ctx.app_state {
    Some(ref shared) => shared.clone(),
    None => Arc::new(RwLock::new(AppState::None)),
  };

  let mut state = MenuState {
    ctx: ctx.clone(),
    app: app.clone(),
    current_menu: Menu::Root,
    menu_options: Vec::new(),
    selected: 0,
    http_message: HttpStatusMessage::Idle,
  };

  state.menu_options = state.get_menu_provider().await.get_items().await;

  let menu_runner = async {
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
                  AppState::HostedApp => { runner_ctx.host_ipc_sender.send((0, crate::protocol::HostIpcMessage::Stop)).await; }
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
                runner_ctx.host_ipc_sender.send((0, crate::protocol::HostIpcMessage::HexButton(hex))).await;
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
  };

  let menu_app_runner = async {
    loop {
      if let MenuAppInput::Start(app_name) = menu_app_input_channel.receive().await {
        *app.write().await = AppState::MenuApp;

        let mut ctx = MenuAppContext::new(
          menu_app_input_channel.receiver(),
          runner_ctx.platform.clone(),
          runner_ctx.host_ipc_sender.clone(),
        );

        // Apply network stack for streaming HTTP if available
        if let Some(ref send_stack) = runner_ctx.network_stack {
          ctx = ctx.with_stack(send_stack.0);
        }

        // Try generic loader first, then custom firmware loader
        match MenuAppType::<P>::load_app_async(&app_name, ctx) {
          Ok(mut menu_app) => {
            menu_app.work().await;
          }
          Err(ctx) => {
            if let Some(loader) = runner_ctx.app_loader {
              loader(app_name, ctx).await;
            }
          }
        }

        // Check WASM_LAUNCHING to avoid drawing menu over a just-launched WASM app
        if WASM_LAUNCHING.swap(false, Ordering::Acquire) {
          *app.write().await = AppState::HostedApp;
        } else {
          *app.write().await = AppState::None;
        }
      }
    }
  };

  join::join(menu_runner, menu_app_runner).await;
}
