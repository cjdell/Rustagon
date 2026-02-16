pub mod execute;
pub mod menus;
pub mod state;
pub mod types;

use crate::{
  apps::{MenuAppAsync, MenuAppContext, MenuAppInput, MenuAppInputChannel, MenuAppType},
  lib::{
    HexButton, HostIpcMessage, HttpStatusMessage, I2cMessage, Icon40, LcdScreen, LedRequest, SystemMessage,
    WasmIpcMessage,
  },
  tasks::wifi::{WifiCommandMessage, WifiDesiredState, WifiStatusMessage},
  utils::{http::perform_http_request_streaming, led_service::LedState, sleep},
};
use alloc::{boxed::Box, format, string::ToString, sync::Arc, vec::Vec};
use core::future::join;
use embassy_futures::{
  select::{Either5, select5},
  yield_now,
};
use embassy_sync::rwlock::RwLock;
use esp_alloc::ExternalMemory;
use esp_println::{print, println};
use log::info;
use menus::MenuProvider as _;
use state::{AppState, MenuState};
use types::{Menu, MenuContext, MenuRunnerContext, WifiStatus};

#[embassy_executor::task]
pub async fn menu_task(mut runner_ctx: MenuRunnerContext) {
  info!("Starting Menu Task...");

  let menu_app_input_channel = mk_static!(MenuAppInputChannel, MenuAppInputChannel::new());

  let ctx = MenuContext {
    stack: runner_ctx.stack,
    local_fs: runner_ctx.local_fs.clone(),
    device_state: runner_ctx.device_state.clone(),
    power_ctrl_sender: runner_ctx.power_ctrl_sender,
    wifi_command_sender: runner_ctx.wifi_command_sender,
    wifi_status_receiver: runner_ctx.wifi_status_receiver,
    wifi_scan_watch: runner_ctx.wifi_scan_watch,
    host_ipc_sender: runner_ctx.host_ipc_sender,
    lcd_signal: runner_ctx.lcd_signal,
    menu_app_input_channel,
  };

  runner_ctx.wifi_command_sender.send(WifiCommandMessage::ChangeState(WifiDesiredState::Online)).await;

  let app = Arc::new(RwLock::new(AppState::None));

  let mut state = MenuState {
    ctx: ctx.clone(),
    app: app.clone(),
    current_menu: Menu::Root,
    menu_options: Vec::new(),
    selected: 0,
    wifi_status: WifiStatus::Offline,
    http_message: HttpStatusMessage::None,
  };

  let menu_runner = async {
    state.menu_options = state.get_menu_provider().get_items().await;

    loop {
      print!("m");
      state.refresh().await;

      match select5(
        runner_ctx.system_receiver.changed(),
        runner_ctx.wifi_status_receiver.receive(),
        runner_ctx.hex_button_subscriber.next_message_pure(),
        runner_ctx.wasm_ipc_channel.receive(),
        runner_ctx.http_event_receiver.receive(),
      )
      .await
      {
        Either5::First(system) => {
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
        Either5::Second(wifi_status) => {
          // println!("Second: {:?}", wifi_status);
          match wifi_status {
            WifiStatusMessage::Connected(ipv4_addr) => {
              state.wifi_status = WifiStatus::Connected(ipv4_addr);
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Wifi, "Connected".to_string()));
              runner_ctx.led_sender.send(LedRequest::Solid(LedState { r: 0, g: 0, b: 255 })).await;
              sleep(2_000).await;
              runner_ctx.led_sender.send(LedRequest::Rainbow).await;
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Info, format!("IP: {}", ipv4_addr)));
              sleep(2_000).await;
            }
            WifiStatusMessage::AccessPointActive => {
              state.wifi_status = WifiStatus::AccessPoint;
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "AP Mode Active".to_string()));
              runner_ctx.led_sender.send(LedRequest::Fire).await;
              sleep(2_000).await;
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(
                Icon40::Info,
                format!("AP: {}", runner_ctx.device_state.get_data().ap_ssid),
              ));
              sleep(2_000).await;
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Info, format!("IP: 192.168.1.1")));
              sleep(2_000).await;
            }
            WifiStatusMessage::NoNetworksFound => {
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Warn, "No Networks Found :-(".to_string()));
              runner_ctx.led_sender.send(LedRequest::Solid(LedState { r: 255, g: 0, b: 0 })).await;
              sleep(1_000).await;
            }
            WifiStatusMessage::Interrupted => {
              state.wifi_status = WifiStatus::Offline;
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Warn, "Interrupted".to_string()));
              runner_ctx.led_sender.send(LedRequest::Solid(LedState { r: 255, g: 0, b: 0 })).await;
              sleep(1_000).await;
            }
            WifiStatusMessage::Disconnected => {
              state.wifi_status = WifiStatus::Offline;
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "Disconnected".to_string()));
              runner_ctx.led_sender.send(LedRequest::Solid(LedState { r: 255, g: 255, b: 0 })).await;
              sleep(1_000).await;
            }
            WifiStatusMessage::Reset => {
              state.wifi_status = WifiStatus::Offline;
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Warn, "Reset".to_string()));
              sleep(1_000).await;
            }
          };

          state.menu_options = state.get_menu_provider().get_items().await;
        }
        Either5::Third(I2cMessage::HexButton(hex)) => {
          // println!("Third: {:?}", hex);
          let app_running =
            app.try_read().map(|app| if let AppState::None = *app { false } else { true }).unwrap_or(true);

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
              HexButton::A => {
                println!("Menu: Pressed A");
                if state.selected > 0 {
                  state.selected -= 1;
                }
              }
              HexButton::B => {
                println!("Menu: Pressed B");
                runner_ctx.led_sender.send(LedRequest::Sparkle(LedState::new(255, 255, 255))).await;
              }
              HexButton::C => {
                println!("Menu: Pressed C");
                runner_ctx.lcd_signal.signal(LcdScreen::Progress("Please wait...".to_string()));
                state.execute_option().await;
              }
              HexButton::D => {
                println!("Menu: Pressed D");
                state.selected += 1;
              }
              HexButton::E => {
                println!("Menu: Pressed E");
              }
              HexButton::F => {
                println!("Menu: Pressed F");
              }
            }
          }
        }
        Either5::Third(_) => {}
        Either5::Fourth((wasm_req_id, wasm_ipc_message)) => {
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
              runner_ctx.lcd_signal.signal(LcdScreen::Headline(Icon40::Info, "App Terminated".to_string()));
              sleep(1_000).await;
            }
            WasmIpcMessage::LcdScreen(lcd_screen) => {
              println!("lcd_screen 1: {:?}", lcd_screen);
              runner_ctx.lcd_signal.signal(lcd_screen);
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
        Either5::Fifth(http_message) => {
          // println!("Fifth: {:?}", http_message);
          match http_message {
            HttpStatusMessage::ReceivedFile(buffer) => {
              menu_app_input_channel.send(MenuAppInput::Stop).await;
              sleep(100).await;

              runner_ctx.host_ipc_sender.send((0, HostIpcMessage::StartWasmWithBuffer(buffer))).await;

              state.http_message = HttpStatusMessage::None;
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
          runner_ctx.local_fs.clone(),
          runner_ctx.device_state.clone(),
          runner_ctx.stack,
          runner_ctx.lcd_signal,
        );

        let mut menu_app = Box::new_in(MenuAppType::load_app_async(app_name, ctx), ExternalMemory);

        menu_app.work().await;

        runner_ctx.wasm_ipc_channel.send((0, WasmIpcMessage::Stopped)).await;
      }
    }
  };

  join!(menu_runner, menu_app_runner).await;
}
