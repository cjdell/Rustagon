pub mod menus;
pub mod state;
pub mod types;

pub use types::*;

use crate::{
  apps::*,
  menu::{
    menus::get_root_menu_options,
    state::*,
  },
  platform::Platform,
  protocol::HostIpcMessage,
  types::*,
};
use alloc::{string::ToString, vec::Vec};
use embassy_futures::select::{select, select3, select4, Either, Either3, Either4};
use log::info;

pub async fn menu_task<P: Platform + 'static>(runner_ctx: MenuRunnerContext<P>) {
  let mut stack: Vec<AppStackEntry<P>> = Vec::new();
  let display = runner_ctx.platform.display_manager();

  stack.push(AppStackEntry::RootMenu {
    menu_options: get_root_menu_options::<P>(runner_ctx.additional_apps),
    selected: 0,
  });

  let stack_signal = runner_ctx.stack_event_handle.clone();

  loop {
    let top_type = stack.last().map(|e| e.entry_type());
    info!("menu_task: top_type={top_type:?} stack.len={}", stack.len());

    match top_type {
      Some(StackEntryType::RootMenu) => {
        info!("menu_task: calling handle_root_menu");
        handle_root_menu(&mut stack, &runner_ctx, &display, &stack_signal).await;
        info!("menu_task: handle_root_menu returned, stack.len={}", stack.len());
      }
      Some(StackEntryType::MenuApp) => {
        info!("menu_task: calling handle_menu_app");
        handle_menu_app(&mut stack, &runner_ctx, &display, &stack_signal).await;
        info!("menu_task: handle_menu_app returned, stack.len={}", stack.len());
      }
      Some(StackEntryType::HostedApp) => {
        info!("menu_task: calling handle_hosted_app");
        handle_hosted_app(&mut stack, &runner_ctx, &display, &stack_signal).await;
        info!("menu_task: handle_hosted_app returned, stack.len={}", stack.len());
      }
      None => {
        info!("menu_task: stack empty, pushing root");
        stack.push(AppStackEntry::RootMenu {
          menu_options: get_root_menu_options::<P>(runner_ctx.additional_apps),
          selected: 0,
        });
      }
    }
  }
}

async fn handle_root_menu<P: Platform>(
  stack: &mut Vec<AppStackEntry<P>>,
  runner_ctx: &MenuRunnerContext<P>,
  display: &crate::platform::DisplayHandle,
  stack_signal: &StackSignal,
) {
  let mut should_render = true;

  loop {
    let idx = stack.len() - 1;

    // Only re-render the menu when the selection changed (skip on hexpansion events
    // so notification overlays from the polling task stay visible)
    if should_render {
      let screen = match &stack[idx] {
        AppStackEntry::RootMenu { menu_options, selected } => {
          LcdScreen::Menu {
            menu: menu_options.iter().map(|option| match option {
              MenuOption::App { name, .. } => MenuLine(Icon20::Info, name.to_string()),
              MenuOption::Back => MenuLine(Icon20::Info, "<= Back".to_string()),
              MenuOption::PowerOff => MenuLine(Icon20::Info, "Power Off".to_string()),
            }).collect(),
            selected: *selected,
          }
        }
        _ => unreachable!(),
      };
      let _ = display.signal(screen);
      should_render = false;
    }

    let input = select3(
      runner_ctx.platform.system_manager().next_button(),
      runner_ctx.platform.input_manager().next_button(),
      select(
        runner_ctx.platform.hexpansion_manager().next_event(),
        runner_ctx.platform.hexpansion_manager().next_device_event(),
      ),
    ).await;

    // Handle root menu navigation
    let mut power_off = false;
    let mut new_entry: Option<AppStackEntry<P>> = None;

    match input {
      Either3::First(_system) => {
        info!("handle_root_menu: boot button — return to main loop");
        return;
      }
      Either3::Second(hex) => {
        should_render = true;
        if let AppStackEntry::RootMenu { menu_options, selected } = &mut stack[idx] {
          match hex {
            HexButton::Up => {
              if *selected > 0 { *selected -= 1; }
            }
            HexButton::Down => {
              if (*selected as usize) < menu_options.len().saturating_sub(1) {
                *selected += 1;
              }
            }
            HexButton::Fire => {
              if *selected as usize >= menu_options.len() { return; }
              match &menu_options[*selected as usize] {
                MenuOption::App { name, app_type } => {
                  match app_type {
                    AppType::MenuApp => {
                      let ctx = MenuAppContext::new(runner_ctx.platform.clone(), runner_ctx.host_ipc_sender);
                      match MenuAppType::<P>::load_app_async(name, ctx) {
                        Ok(mut app) => {
                          app.init().await;
                          let _ = display.signal(app.render());
                          new_entry = Some(AppStackEntry::MenuApp { app });
                        }
                        Err(ctx) => {
                          if let Some(loader) = runner_ctx.app_loader {
                            loader(name.to_string(), ctx).await;
                            new_entry = Some(AppStackEntry::HostedApp);
                          }
                        }
                      }
                    }
                    AppType::NativeApp => {
                      runner_ctx.host_ipc_sender.send((
                        0,
                        HostIpcMessage::StartNative(name.to_string()),
                      )).await;
                      new_entry = Some(AppStackEntry::HostedApp);
                    }
                  }
                }
                MenuOption::PowerOff => {
                  power_off = true;
                }
                MenuOption::Back => {}
              }
            }
            _ => {}
          }

          // Clamp selected
          if *selected as usize >= menu_options.len() {
            *selected = menu_options.len().saturating_sub(1) as u32;
          }
        }
      }
      Either3::Third(inner) => match inner {
        Either::First(hx_event) => {
          info!("handle_root_menu: hexpansion event {hx_event:?}");
          if let Some(event) = stack_signal.try_receive() {
            info!("handle_root_menu: stack event {event:?}");
            match event {
              StackEvent::Pushed(StackEntryType::HostedApp) => stack.push(AppStackEntry::HostedApp),
              StackEvent::Popped => { if stack.len() > 1 { let _ = stack.pop(); } }
              _ => {}
            }
          }
        }
        Either::Second(dev_event) => {
          info!("handle_root_menu: device event {dev_event:?}");
          // Inject navigation keys as HexButton for menu navigation
          if let DeviceEvent::Keyboard(ke) = &dev_event {
            if ke.typ == KeyEventType::Pressed {
              if let Some(nav) = device_key_to_nav(ke.code) {
                runner_ctx.platform.input_manager().inject_button(nav).await;
              }
            }
          }
        }
      }
    }

    // Apply stack mutations (borrow on stack is released)
    if let Some(entry) = new_entry {
      stack.push(entry);
      return;
    }

    // Non-blocking check for stack events
    if let Some(event) = stack_signal.try_receive() {
      info!("handle_root_menu: stack event {event:?}");
      match event {
        StackEvent::Pushed(StackEntryType::HostedApp) => stack.push(AppStackEntry::HostedApp),
        StackEvent::Popped => { if stack.len() > 1 { let _ = stack.pop(); } }
        _ => {}
      }
    }

    if power_off {
      runner_ctx.platform.power_manager().power_off().await;
    }
  }
}

async fn handle_menu_app<P: Platform>(
  stack: &mut Vec<AppStackEntry<P>>,
  runner_ctx: &MenuRunnerContext<P>,
  display: &crate::platform::DisplayHandle,
  stack_signal: &StackSignal,
) {
  let idx = stack.len() - 1;

  if let AppStackEntry::MenuApp { app } = &stack[idx] {
    let _ = display.signal(app.render());
  }

  let event = select4(
    runner_ctx.platform.system_manager().next_button(),
    runner_ctx.platform.input_manager().next_button(),
    stack_signal.receive(),
    select(
      runner_ctx.platform.hexpansion_manager().next_event(),
      runner_ctx.platform.hexpansion_manager().next_device_event(),
    ),
  ).await;

  match event {
    Either4::First(_system) => {
      info!("handle_menu_app: boot button — popping app");
      let _ = stack.pop();
    }
    Either4::Second(hex) => {
      info!("handle_menu_app: hex button {hex:?}");
      let action = if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
        app.handle_input(MenuAppInput::Button(hex)).await
      } else {
        AppAction::Continue
      };
      info!("handle_menu_app: action={action:?}");

      match action {
        AppAction::Continue => {}
        AppAction::Stop => {
          let _ = stack.pop();
          info!("handle_menu_app: popped app (stop)");
        }
        AppAction::LaunchWasm(name) => {
          info!("handle_menu_app: launching wasm {name}");
          runner_ctx.host_ipc_sender.send((
            0,
            HostIpcMessage::StartWasm(name),
          )).await;
          stack.push(AppStackEntry::HostedApp);
        }
        AppAction::LaunchNative(name) => {
          info!("handle_menu_app: launching native {name}");
          runner_ctx.host_ipc_sender.send((
            0,
            HostIpcMessage::StartNative(name),
          )).await;
          stack.push(AppStackEntry::HostedApp);
        }
      }
    }
    Either4::Third(event) => {
      info!("handle_menu_app: stack event {event:?}");
      match event {
        StackEvent::Popped => {
          if stack.len() > 1 { let _ = stack.pop(); }
        }
        StackEvent::Pushed(StackEntryType::HostedApp) => {
          stack.push(AppStackEntry::HostedApp);
        }
        _ => {}
      }
    }
    Either4::Fourth(inner) => match inner {
      Either::First(hx_event) => {
        info!("handle_menu_app: hexpansion event {hx_event:?}");
        if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
          app.handle_event(AppEvent::Hexpansion(hx_event)).await;
        }
      }
      Either::Second(dev_event) => {
        info!("handle_menu_app: device event {dev_event:?}");
        if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
          app.handle_event(AppEvent::Device(dev_event)).await;
        }
      }
    }
  }

  // Drain any remaining device events (non-blocking)
  while let Some(dev_event) = runner_ctx.platform.hexpansion_manager().try_next_device_event() {
    info!("handle_menu_app: drain device event {dev_event:?}");
    if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
      app.handle_event(AppEvent::Device(dev_event)).await;
    }
  }
}

async fn handle_hosted_app<P: Platform>(
  stack: &mut Vec<AppStackEntry<P>>,
  runner_ctx: &MenuRunnerContext<P>,
  display: &crate::platform::DisplayHandle,
  stack_signal: &StackSignal,
) {
  loop {
    let input = embassy_futures::select::select4(
      runner_ctx.platform.system_manager().next_button(),
      runner_ctx.platform.input_manager().next_button(),
      stack_signal.receive(),
      select(
        runner_ctx.platform.hexpansion_manager().next_event(),
        runner_ctx.platform.hexpansion_manager().next_device_event(),
      ),
    ).await;

    match input {
      Either4::First(_system) => {
        runner_ctx.host_ipc_sender.try_send((0, HostIpcMessage::Stop)).ok();
      }
      Either4::Second(hex) => {
        runner_ctx.host_ipc_sender.try_send((0, HostIpcMessage::HexButton(hex))).ok();
      }
      Either4::Third(event) => {
        info!("hosted_app: stack event {event:?}");
        match event {
          StackEvent::Popped => {
            if stack.len() > 1 {
              let _ = stack.pop();
            }
            return;
          }
          StackEvent::Pushed(typ) => {
            match typ {
              StackEntryType::HostedApp => stack.push(AppStackEntry::HostedApp),
              _ => {}
            }
          }
        }
      }
      Either4::Fourth(inner) => match inner {
        Either::First(hx_event) => {
          info!("hosted_app: hexpansion event {hx_event:?}");
        }
        Either::Second(dev_event) => {
          info!("hosted_app: device event {dev_event:?}");
          // Inject navigation keys as HexButton for hosted apps
          if let DeviceEvent::Keyboard(ke) = &dev_event {
            if ke.typ == KeyEventType::Pressed {
              if let Some(nav) = device_key_to_nav(ke.code) {
                runner_ctx.host_ipc_sender.try_send((0, HostIpcMessage::HexButton(nav))).ok();
              }
            }
          }
        }
      }
    }
  }
}

/// Map a key event's KeyCode to a HexButton for navigation.
fn device_key_to_nav(code: KeyCode) -> Option<HexButton> {
  match code {
    KeyCode::Up => Some(HexButton::Up),
    KeyCode::Down => Some(HexButton::Down),
    KeyCode::Left => Some(HexButton::Left),
    KeyCode::Right => Some(HexButton::Right),
    KeyCode::Enter => Some(HexButton::Fire),
    KeyCode::Escape => Some(HexButton::HexF),
    KeyCode::Tab => Some(HexButton::HexE),
    _ => None,
  }
}
