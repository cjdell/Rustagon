pub mod execute;
pub mod menus;
pub mod state;
pub mod types;

pub use types::*;

use crate::{apps::*, platform::Platform, protocol::*, types::*, utils::*};
use alloc::{boxed::Box, format, string::ToString, sync::Arc, vec::Vec};
use core::future::join;
use embassy_futures::{
  select::{Either4, select4},
  yield_now,
};
use embassy_sync::rwlock::RwLock;
use esp_alloc::ExternalMemory;
use esp_println::{print, println};
use log::info;
use menus::MenuProvider as _;
use picoserve::make_static;
use state::{AppState, MenuState};

#[embassy_executor::task]
pub async fn menu_task(mut runner_ctx: MenuRunnerContext) {
  info!("Starting Menu Task...");

  let menu_app_input_channel = make_static!(MenuAppInputChannel, MenuAppInputChannel::new());

  let ctx = MenuContext {
    stack: runner_ctx.stack,
    storage: runner_ctx.storage.clone(),
    platform: runner_ctx.platform.clone(),
    host_ipc_sender: runner_ctx.host_ipc_sender,
    display: runner_ctx.platform.display_manager(),
    menu_app_input_channel,
  };

  runner_ctx.platform.wifi_manager().set_desired_state(crate::platform::WifiDesiredState::Online).await;

  let app = Arc::new(RwLock::new(AppState::None));

  let mut state = MenuState {
    ctx: ctx.clone(),
    app: app.clone(),
    current_menu: Menu::Root,
    menu_options: Vec::new(),
    selected: 0,
    wifi_status: WifiStatus::Offline,
    http_message: HttpStatusMessage::Idle,
  };

  let menu_runner = async {
    state.menu_options = state.get_menu_provider().await.get_items().await;

    loop {
      print!("m");
      state.refresh().await;

      match select4(
        runner_ctx.platform.system_manager().next_button(),
        runner_ctx.platform.input_manager().next_button(),
        runner_ctx.wasm_ipc_channel.receive(),
        runner_ctx.http_event_receiver.receive(),
      )
      .await
      {
        Either4::First(system) => {
          // println!("First: {:?}", system);
          match system {
            SystemMessage::BootButton => {
              if let Ok(app) = state.app.try_read() {
                match *app {
                  AppState::MenuApp => {
                    menu_app_input_channel.send(MenuAppInput::Stop).await;
                  }
                  AppState::HostedApp => {
                    runner_ctx.host_ipc_sender.send((0, HostIpcMessage::Stop)).await;
                  }
                  _ => (),
                }
              }
            }
          };
        }
        Either4::Second(hex) => {
          println!("Button: {:?}", hex);
          let app_running = app.try_read().map(|app| if let AppState::None = *app { false } else { true }).unwrap_or(true);

          if app_running {
            match *app.write().await {
              AppState::MenuApp => {
                menu_app_input_channel.send(MenuAppInput::HexButton(hex.clone())).await;
                continue;
              }
              AppState::HostedApp => {
                runner_ctx.host_ipc_sender.send((0, HostIpcMessage::HexButton(hex))).await;
                continue;
              }
              _ => {}
            };
          } else {
            match hex {
              HexButton::Up => {
                if state.selected > 0 {
                  state.selected -= 1;
                }
              }
              HexButton::Right => {
                let _ = runner_ctx.platform.led_manager().request(LedRequest::Sparkle(LedState::new(255, 255, 255)));
              }
              HexButton::Fire => {
                let _ = runner_ctx.platform.display_manager().signal(LcdScreen::Progress("Please wait...".to_string()));
                state.execute_option().await;
              }
              HexButton::Down => {
                state.selected += 1;
              }
              HexButton::Left => {}
              HexButton::HexA => {}
              HexButton::HexB => {}
              HexButton::HexC => {}
              HexButton::HexD => {}
              HexButton::HexE => {}
              HexButton::HexF => {}
            }
          }
        }
        Either4::Third((wasm_req_id, wasm_ipc_message)) => {
          // println!("Fourth: {:?}", wasm_ipc_message);
          match wasm_ipc_message {
            WasmIpcMessage::Started => {
              *state.app.write().await = AppState::HostedApp;
            }
            WasmIpcMessage::MenuAppStarted => {
              *state.app.write().await = AppState::MenuApp;
            }
            WasmIpcMessage::Stopped => {
              *state.app.write().await = AppState::None;
              let _ = runner_ctx.platform.display_manager().signal(LcdScreen::Headline(Icon40::Info, "App Terminated".to_string()));
              sleep(1_000).await;
            }
            WasmIpcMessage::LcdScreen(lcd_screen) => {
              println!("lcd_screen 1: {:?}", lcd_screen);
              let _ = runner_ctx.platform.display_manager().signal(lcd_screen);
              yield_now().await;
            }
            WasmIpcMessage::HttpRequest(http_request) => {
              match perform_http_request_streaming(
                runner_ctx.stack,
                &http_request,
                |meta| ctx.host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpResponseMeta(meta))),
                |chunk| ctx.host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpResponseBody(chunk))),
              )
              .await
              {
                Ok(()) => {
                  ctx.host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpResponseComplete)).await;
                }
                Err(()) => {
                  runner_ctx.host_ipc_sender.send((wasm_req_id, HostIpcMessage::HttpError)).await;
                }
              }
            }
          };
        }
        Either4::Fourth(http_message) => {
          // println!("Fifth: {:?}", http_message);
          match http_message {
            HttpStatusMessage::ReceivedFile(buffer) => {
              menu_app_input_channel.send(MenuAppInput::Stop).await;
              sleep(100).await;

              runner_ctx.host_ipc_sender.send((0, HostIpcMessage::StartWasmWithBuffer(buffer))).await;

              state.http_message = HttpStatusMessage::Idle;
            }
            _ => {
              state.http_message = http_message;
            }
          };
        }
      };
    }
  };

  let menu_app_runner = async {
    loop {
      if let MenuAppInput::Start(app_name) = menu_app_input_channel.receive().await {
        runner_ctx.wasm_ipc_channel.send((0, WasmIpcMessage::MenuAppStarted)).await;

        let ctx = MenuAppContext::new(
          menu_app_input_channel.receiver(),
          runner_ctx.stack,
          runner_ctx.platform.clone(),
        );

        let mut menu_app = Box::new_in(MenuAppType::load_app_async(app_name, ctx), ExternalMemory);

        menu_app.work().await;

        runner_ctx.wasm_ipc_channel.send((0, WasmIpcMessage::Stopped)).await;
      }
    }
  };

  join!(menu_runner, menu_app_runner).await;
}
