pub mod menus;
pub mod state;
pub mod types;

pub use types::*;

use crate::{
  apps::*,
  menu::{menus::get_root_menu_options, state::*},
  platform::Platform,
  protocol::{HostIpcMessage, HostRuntimeCommand},
  types::*,
};
use alloc::{string::ToString, vec::Vec};
use embassy_futures::select::{Either, Either3, Either4, select, select3, select4};
use log::{debug, info};
use wasm_protocol::HostIpcMessage as WireHostIpcMessage;

pub async fn menu_task<P: Platform + 'static>(mut runner_ctx: MenuRunnerContext<P>) {
  let mut stack: Vec<AppStackEntry<P>> = Vec::new();
  let display = runner_ctx.platform.display_manager();

  stack.push(AppStackEntry::RootMenu {
    menu_options: get_root_menu_options::<P>(runner_ctx.additional_apps),
    selected: 0,
  });

  // Auto-launch a WASM app (e.g. desktop CLI arg) before entering the menu loop
  if let Some(buffer) = runner_ctx.auto_launch.take() {
    info!("menu_task: auto-launching WASM app ({} bytes)", buffer.len());
    runner_ctx
      .host_ipc_sender
      .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartWasmWithBuffer(buffer))))
      .await;
    stack.push(AppStackEntry::HostedApp);
  }

  let stack_signal = runner_ctx.stack_event_handle.clone();

  loop {
    let top_type = stack.last().map(|e| e.entry_type());
    debug!("menu_task: top_type={top_type:?} stack.len={}", stack.len());

    match top_type {
      Some(StackEntryType::RootMenu) => {
        debug!("menu_task: calling handle_root_menu");
        handle_root_menu(&mut stack, &runner_ctx, &display, &stack_signal).await;
        debug!("menu_task: handle_root_menu returned, stack.len={}", stack.len());
      }
      Some(StackEntryType::MenuApp) => {
        debug!("menu_task: calling handle_menu_app");
        handle_menu_app(&mut stack, &runner_ctx, &display, &stack_signal).await;
        debug!("menu_task: handle_menu_app returned, stack.len={}", stack.len());
      }
      Some(StackEntryType::HostedApp) => {
        debug!("menu_task: calling handle_hosted_app");
        handle_hosted_app(&mut stack, &runner_ctx, &display, &stack_signal).await;
        debug!("menu_task: handle_hosted_app returned, stack.len={}", stack.len());
      }
      None => {
        debug!("menu_task: stack empty, pushing root");
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
        AppStackEntry::RootMenu { menu_options, selected } => LcdScreen::Menu {
          menu: menu_options
            .iter()
            .map(|option| match option {
              MenuOption::App { name, .. } => MenuLine(Icon20::Info, name.to_string()),
              MenuOption::Back => MenuLine(Icon20::Info, "<= Back".to_string()),
              MenuOption::PowerOff => MenuLine(Icon20::Info, "Power Off".to_string()),
            })
            .collect(),
          selected: *selected,
          // Returning to the root menu is a "back" transition: slide in from
          // the left (content moves rightward) to mirror the forward push.
          animation: MenuAnimation::FromLeft,
        },
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
    )
    .await;

    // Handle root menu navigation
    let mut power_off = false;
    let mut new_entry: Option<AppStackEntry<P>> = None;

    match input {
      Either3::First(_system) => {
        debug!("handle_root_menu: boot button — return to main loop");
        return;
      }
      Either3::Second(hex) => {
        should_render = true;
        if let AppStackEntry::RootMenu { menu_options, selected } = &mut stack[idx] {
          match hex {
            HexButton::Up => {
              if *selected > 0 {
                *selected -= 1;
              }
            }
            HexButton::Down => {
              if (*selected as usize) < menu_options.len().saturating_sub(1) {
                *selected += 1;
              }
            }
            HexButton::Fire => {
              if *selected as usize >= menu_options.len() {
                return;
              }
              match &menu_options[*selected as usize] {
                MenuOption::App { name, app_type } => match app_type {
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
                    runner_ctx
                      .host_ipc_sender
                      .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartNative(name.to_string()))))
                      .await;
                    new_entry = Some(AppStackEntry::HostedApp);
                  }
                },
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
          debug!("handle_root_menu: hexpansion event {hx_event:?}");
          if let Some(event) = stack_signal.try_receive() {
            debug!("handle_root_menu: stack event {event:?}");
            match event {
              StackEvent::Pushed(StackEntryType::HostedApp) => stack.push(AppStackEntry::HostedApp),
              StackEvent::Popped => {
                if stack.len() > 1 {
                  let _ = stack.pop();
                }
              }
              _ => {}
            }
          }
        }
        Either::Second(dev_event) => {
          debug!("handle_root_menu: device event {dev_event:?}");
          // Inject navigation keys as HexButton (press and release) for menu navigation
          if let DeviceEvent::Keyboard(ke) = &dev_event {
            if let Some(nav) = device_key_to_nav(ke.code) {
              let button = match ke.typ {
                KeyEventType::Pressed => nav,
                KeyEventType::Released => nav.released(),
              };
              runner_ctx.platform.input_manager().inject_button(button).await;
            }
          }
        }
      },
    }

    // Apply stack mutations (borrow on stack is released)
    if let Some(entry) = new_entry {
      stack.push(entry);
      return;
    }

    // Non-blocking check for stack events
    if let Some(event) = stack_signal.try_receive() {
      debug!("handle_root_menu: stack event {event:?}");
      match event {
        StackEvent::Pushed(StackEntryType::HostedApp) => stack.push(AppStackEntry::HostedApp),
        StackEvent::Popped => {
          if stack.len() > 1 {
            let _ = stack.pop();
          }
        }
        _ => {}
      }
    }

    if power_off {
      runner_ctx.platform.power_manager().power_off().await;
    }
  }
}

/// Cadence at which foreground apps receive `MenuApp::tick()`, letting them
/// drain background channels (TCP, HTTP) without waiting for user input.
pub const APP_TICK_MS: u64 = 50;

/// A single event from the menu's multiplexed input sources, flattened into
/// one enum so app handling doesn't need to reason about nested `Either`s.
enum MenuEvent {
  System,
  Button(HexButton),
  Stack(StackEvent),
  Hexpansion(HexpansionEvent),
  Device(DeviceEvent),
  Tick,
}

/// Wait for the next menu-source event. A [`APP_TICK_MS`] timer is multiplexed
/// in so the menu wakes periodically even when the user does nothing — apps get
/// a `tick` cadence for background work.
async fn next_menu_event<P: Platform>(platform: &P, stack_signal: &StackSignal) -> MenuEvent {
  let event = select4(
    platform.system_manager().next_button(),
    platform.input_manager().next_button(),
    stack_signal.receive(),
    select(
      select(
        platform.hexpansion_manager().next_event(),
        platform.hexpansion_manager().next_device_event(),
      ),
      crate::utils::sleep(APP_TICK_MS),
    ),
  )
  .await;
  match event {
    Either4::First(_) => MenuEvent::System,
    Either4::Second(button) => MenuEvent::Button(button),
    Either4::Third(stack) => MenuEvent::Stack(stack),
    Either4::Fourth(Either::First(Either::First(hx))) => MenuEvent::Hexpansion(hx),
    Either4::Fourth(Either::First(Either::Second(dev))) => MenuEvent::Device(dev),
    Either4::Fourth(Either::Second(_)) => MenuEvent::Tick,
  }
}

async fn handle_menu_app<P: Platform>(
  stack: &mut Vec<AppStackEntry<P>>,
  runner_ctx: &MenuRunnerContext<P>,
  display: &crate::platform::DisplayHandle,
  stack_signal: &StackSignal,
) {
  let idx = stack.len() - 1;
  if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
    // Entering (or re-entering after a sub-app popped): give the app a chance
    // to refresh, then show its current screen.
    app.on_shown().await;
    let _ = display.signal(app.render());
  }

  loop {
    let idx = stack.len() - 1;
    let event = next_menu_event(&runner_ctx.platform, stack_signal).await;

    // Background cadence: let the app drain its channels and do periodic work
    // without waiting for user input.
    if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
      app.tick().await;
    }

    let mut pop = false;
    let mut push_hosted = false;
    let mut action = AppAction::Continue;
    // Capture before the match: the arms move `event`'s payloads.
    let is_tick = matches!(event, MenuEvent::Tick);

    match event {
      MenuEvent::System => {
        debug!("handle_menu_app: boot button — popping app");
        pop = true;
      }
      MenuEvent::Button(hex) => {
        debug!("handle_menu_app: hex button {hex:?}");
        action = if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
          app.handle_input(MenuAppInput::Button(hex)).await
        } else {
          AppAction::Continue
        };
        debug!("handle_menu_app: action={action:?}");
      }
      MenuEvent::Stack(stack_event) => {
        debug!("handle_menu_app: stack event {stack_event:?}");
        match stack_event {
          StackEvent::Popped => {
            if stack.len() > 1 {
              pop = true;
            }
          }
          StackEvent::Pushed(StackEntryType::HostedApp) => push_hosted = true,
          _ => {}
        }
      }
      MenuEvent::Hexpansion(hx_event) => {
        debug!("handle_menu_app: hexpansion event {hx_event:?}");
        if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
          app.handle_event(AppEvent::Hexpansion(hx_event)).await;
        }
      }
      MenuEvent::Device(dev_event) => {
        debug!("handle_menu_app: device event {dev_event:?}");
        if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
          app.handle_event(AppEvent::Device(dev_event)).await;
        }
      }
      MenuEvent::Tick => {}
    }

    // Drain any remaining device events (non-blocking)
    while let Some(dev_event) = runner_ctx.platform.hexpansion_manager().try_next_device_event() {
      debug!("handle_menu_app: drain device event {dev_event:?}");
      if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
        app.handle_event(AppEvent::Device(dev_event)).await;
      }
    }

    if push_hosted {
      stack.push(AppStackEntry::HostedApp);
      return;
    }

    match action {
      AppAction::Continue => {}
      AppAction::Stop => pop = true,
      AppAction::LaunchWasm(name) => {
        debug!("handle_menu_app: launching wasm {name}");
        runner_ctx
          .host_ipc_sender
          .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartWasm(name))))
          .await;
        stack.push(AppStackEntry::HostedApp);
        return;
      }
      AppAction::LaunchNative(name) => {
        debug!("handle_menu_app: launching native {name}");
        runner_ctx
          .host_ipc_sender
          .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartNative(name))))
          .await;
        stack.push(AppStackEntry::HostedApp);
        return;
      }
    }

    if pop {
      if let AppStackEntry::MenuApp { app } = &mut stack[idx] {
        app.on_stop().await;
      }
      let _ = stack.pop();
      debug!("handle_menu_app: popped app");
      return;
    }

    // Re-render the app's screen after a real input/event. Skip ticks so idle
    // apps don't flood the display (they may have already updated it in tick).
    if !is_tick {
      if let AppStackEntry::MenuApp { app } = &stack[idx] {
        let _ = display.signal(app.render());
      }
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
    )
    .await;

    match input {
      Either4::First(_system) => {
        runner_ctx
          .host_ipc_sender
          .try_send((0, HostIpcMessage::Runtime(HostRuntimeCommand::Stop)))
          .ok();
      }
      Either4::Second(hex) => {
        runner_ctx
          .host_ipc_sender
          .try_send((0, HostIpcMessage::Wire(WireHostIpcMessage::HexButton(hex))))
          .ok();
      }
      Either4::Third(event) => {
        debug!("hosted_app: stack event {event:?}");
        match event {
          StackEvent::Popped => {
            if stack.len() > 1 {
              let _ = stack.pop();
            }
            return;
          }
          StackEvent::Pushed(typ) => match typ {
            StackEntryType::HostedApp => stack.push(AppStackEntry::HostedApp),
            _ => {}
          },
        }
      }
      Either4::Fourth(inner) => match inner {
        Either::First(hx_event) => {
          debug!("hosted_app: hexpansion event {hx_event:?}");
        }
        Either::Second(dev_event) => {
          debug!("hosted_app: device event {dev_event:?}");
          // Forward navigation keys as HexButton (press and release) to hosted apps
          if let DeviceEvent::Keyboard(ke) = &dev_event {
            if let Some(nav) = device_key_to_nav(ke.code) {
              let button = match ke.typ {
                KeyEventType::Pressed => nav,
                KeyEventType::Released => nav.released(),
              };
              runner_ctx
                .host_ipc_sender
                .try_send((0, HostIpcMessage::Wire(WireHostIpcMessage::HexButton(button))))
                .ok();
            }
          }
        }
      },
    }
  }
}

/// Map a key event's KeyCode to a HexButton for navigation.
///
/// Arrows and Enter are intentionally absent: the platform already surfaces
/// them as `HexButton` presses (firmware keyboard driver, desktop key mapping),
/// so they never arrive here as separate keyboard events.
fn device_key_to_nav(code: KeyCode) -> Option<HexButton> {
  match code {
    KeyCode::Escape => Some(HexButton::HexF),
    KeyCode::Tab => Some(HexButton::HexE),
    _ => None,
  }
}
