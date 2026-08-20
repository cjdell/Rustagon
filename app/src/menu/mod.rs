//! The menu: a stack of entries, each driven to completion by a single
//! `run_entry`-style function.
//!
//! - **RootMenu** and **MenuApp** entries run the app's own loop
//!   (`MenuApp::run`) through an [`AppRunContext`]; the menu only selects on
//!   the app's future against the stack signal (sub-app push/pop from the
//!   hosted-app runtime) and translates the returned [`AppAction`].
//! - **HostedApp** entries run a pump that forwards input to the WASM/native
//!   runtime and watches for the pop.
//!
//! There are no per-event `handle_input`/`handle_event`/`tick` callbacks and
//! no nested `select3`/`select4` here — apps own their loops, and the only
//! multiplexing is the single `select` inside [`AppRunContext`].

pub mod menus;
pub mod state;
pub mod types;

pub use types::*;

use crate::{
  apps::{
    nav_button_from_device_event, AppAction, AppInput, AppParams, AppResult, AppRunContext, AppRunEvent, MenuApp, MenuAppContext,
    MenuAppType, ResultChannel,
  },
  menu::{menus::get_root_menu_options, state::*},
  platform::{DisplayHandle, Platform},
  protocol::{HostIpcMessage, HostRuntimeCommand},
  types::{HexButton, Icon20, MenuAnimation, MenuLine},
  ui::list::List,
};
use alloc::{boxed::Box, string::ToString, vec::Vec};
use display_types::LcdScreen;
use embassy_futures::select::{select, Either};
use embassy_sync::channel::Channel;
use log::{debug, info, warn};
use wasm_protocol::HostIpcMessage as WireHostIpcMessage;

/// What a menu-app entry did when it returned.
enum MenuAppOutcome<P: Platform> {
  /// The app stopped (boot button or `Stop` action) or returned an explicit
  /// result (a sub-app popped after `AppAction::Push`); pop it. `result` is
  /// `Some` only when the app returned `AppAction::Result(..)`; a plain `Stop`
  /// of a pushed child is delivered to the parent as `Cancelled`.
  Popped { result: Option<AppResult> },
  /// A hosted app must run; push a `HostedApp` entry.
  Hosted,
  /// A built-in sub-app was loaded; push it on top. Boxed to keep the enum
  /// small: `MenuAppType` carries an entire app (platform context and app
  /// state), and the menu stack already heap-owns it. `result_channel` is
  /// set for `AppAction::Push` and must be attached to the launcher entry
  /// (the current stack top) so the child's result is delivered on pop.
  Loaded(Box<MenuAppType<P>>, Option<ResultChannel>),
}

/// Deliver a just-popped sub-app's result to the new stack top (the
/// launcher), if it has a pending result channel. A `None` result (the
/// child stopped without an explicit `Result`) is delivered as
/// [`AppResult::Cancelled`] so the launcher never hangs waiting for its
/// push to resolve. The channel has capacity 1 and no receiver is polling
/// it yet, so `try_send` always succeeds and the value is buffered until
/// the launcher re-enters `run()` (delivered as `AppRunEvent::Result`).
fn deliver_child_result<P: Platform>(stack: &mut [AppStackEntry<P>], result: Option<AppResult>) {
  match stack.last_mut() {
    Some(AppStackEntry::MenuApp {
      pending_result: Some(channel),
      ..
    }) => {
      let _ = channel.try_send(result.unwrap_or(AppResult::Cancelled));
    }
    _ => {
      if let Some(result) = result {
        debug!("menu_task: child result {result:?} dropped (no menu-app parent to receive it)");
      }
    }
  }
}

pub async fn menu_task<P: Platform + 'static>(mut runner_ctx: MenuRunnerContext<P>) {
  let display = runner_ctx.platform.display_manager();

  let mut stack: Vec<AppStackEntry<P>> = Vec::new();
  stack.push(AppStackEntry::RootMenu {
    menu: RootMenuApp::new(
      MenuAppContext::new(runner_ctx.platform.clone(), runner_ctx.host_ipc_sender),
      get_root_menu_options::<P>(runner_ctx.additional_apps),
    ),
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

  loop {
    let top_type = stack.last().map(|e| e.entry_type());
    debug!("menu_task: top_type={top_type:?} stack.len={}", stack.len());

    match top_type {
      Some(StackEntryType::RootMenu) => run_root_menu(&mut stack, &runner_ctx, &display).await,
      Some(StackEntryType::MenuApp) => match run_menu_app(&mut stack, &runner_ctx, &display).await {
        MenuAppOutcome::Popped { result } => {
          let _ = stack.pop();
          deliver_child_result(&mut stack, result);
        }
        MenuAppOutcome::Hosted => stack.push(AppStackEntry::HostedApp),
        MenuAppOutcome::Loaded(app, result_channel) => {
          if let Some(channel) = result_channel {
            // `run_menu_app` only produces `Loaded` for a MenuApp entry, so
            // the current top is the launcher; it awaits the child's result
            // on this channel after the child pops.
            if let Some(AppStackEntry::MenuApp { pending_result, .. }) = stack.last_mut() {
              *pending_result = Some(channel);
            }
          }
          stack.push(AppStackEntry::MenuApp {
            app: *app,
            pending_result: None,
          });
        }
      },
      Some(StackEntryType::HostedApp) => run_hosted_app(&mut stack, &runner_ctx).await,
      None => {
        debug!("menu_task: stack empty, pushing root");
        stack.push(AppStackEntry::RootMenu {
          menu: RootMenuApp::new(
            MenuAppContext::new(runner_ctx.platform.clone(), runner_ctx.host_ipc_sender),
            get_root_menu_options::<P>(runner_ctx.additional_apps),
          ),
        });
      }
    }
  }
}

/// The root menu, as an ordinary app: it owns its options list (selection
/// included, via [`ui::list::List`]), drives its own loop, and launches apps
/// via [`AppAction`].
pub struct RootMenuApp<P: Platform> {
  ctx: MenuAppContext<P>,
  list: List<MenuOption>,
}

impl<P: Platform> RootMenuApp<P> {
  pub fn new(ctx: MenuAppContext<P>, options: Vec<MenuOption>) -> Self {
    Self {
      ctx,
      list: List::new(options),
    }
  }
}

impl<P: Platform> MenuApp<P> for RootMenuApp<P> {
  fn render(&self) -> LcdScreen {
    LcdScreen::Menu {
      menu: self
        .list
        .items()
        .iter()
        .map(|option| match option {
          MenuOption::App { name, .. } => MenuLine(Icon20::Info, name.to_string()),
          MenuOption::Back => MenuLine(Icon20::Info, "<= Back".to_string()),
          MenuOption::PowerOff => MenuLine(Icon20::Info, "Power Off".to_string()),
        })
        .collect(),
      selected: self.list.selected() as u32,
      // Returning to the root menu is a "back" transition: slide in from
      // the left (content moves rightward) to mirror the forward push.
      animation: MenuAnimation::FromLeft,
    }
  }

  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction {
    loop {
      let event = ctx.next().await;
      // The boot button at the root is a no-op (matches the old handler):
      // there is nothing to quit.
      let AppRunEvent::Input(AppInput::Button(hex)) = event else {
        continue;
      };

      let mut launched: Option<AppAction> = None;
      match hex {
        HexButton::Up => self.list.move_up(),
        HexButton::Down => self.list.move_down(),
        HexButton::Fire => {
          if let Some(option) = self.list.selected_item() {
            launched = Some(match option {
              MenuOption::App { name, app_type } => match app_type {
                AppType::MenuApp => AppAction::LoadMenuApp((*name).to_string()),
                AppType::NativeApp => AppAction::LaunchNative(name.to_string()),
              },
              MenuOption::PowerOff => {
                self.ctx.platform.power_manager().power_off().await;
                AppAction::Continue
              }
              MenuOption::Back => AppAction::Continue,
            });
          }
        }
        _ => {}
      }

      // Re-render on button events. Hexpansion/device events deliberately do
      // not re-render so notification overlays from the polling task stay
      // visible.
      self.ctx.update_lcd(self.render());

      if let Some(action) = launched {
        return action;
      }
    }
  }
}

/// Run the root menu until it launches something (or a hosted app is pushed
/// on top of it by the runtime). Returns with the stack already mutated.
async fn run_root_menu<P: Platform>(stack: &mut Vec<AppStackEntry<P>>, runner_ctx: &MenuRunnerContext<P>, display: &DisplayHandle) {
  let idx = stack.len() - 1;
  let AppStackEntry::RootMenu { menu } = &mut stack[idx] else {
    unreachable!("run_root_menu only dispatches on RootMenu entries");
  };
  let _ = display.signal(menu.render());

  let ctx = AppRunContext::new(&runner_ctx.platform, None);
  let action = select(menu.run(ctx), runner_ctx.stack_event_handle.receive()).await;

  match action {
    Either::Second(StackEvent::Pushed(_)) => {
      // A hosted app was launched from outside (HTTP upload); run it.
      stack.push(AppStackEntry::HostedApp);
    }
    Either::Second(StackEvent::Popped) => {
      // Defensive stale pop at the bottom of the stack; nothing to do.
    }
    Either::First(action) => match action {
      AppAction::LoadMenuApp(name) => {
        let mctx = MenuAppContext::new(runner_ctx.platform.clone(), runner_ctx.host_ipc_sender);
        match MenuAppType::<P>::load_app_async(&name, mctx, AppParams::None) {
          Ok(app) => stack.push(AppStackEntry::MenuApp { app, pending_result: None }),
          Err(mctx) => {
            // Not a built-in: hand it to the platform's app loader (WASM).
            if let Some(loader) = runner_ctx.app_loader {
              loader(name, mctx).await;
            }
            stack.push(AppStackEntry::HostedApp);
          }
        }
      }
      AppAction::Push(name, _params) => {
        // The root menu has no pending slot, so a pushed child's result
        // would be dropped; degrade to a plain launch.
        warn!("run_root_menu: Push of {name} at the root cannot deliver a result");
        let mctx = MenuAppContext::new(runner_ctx.platform.clone(), runner_ctx.host_ipc_sender);
        match MenuAppType::<P>::load_app_async(&name, mctx, AppParams::None) {
          Ok(app) => stack.push(AppStackEntry::MenuApp { app, pending_result: None }),
          Err(_) => stack.push(AppStackEntry::HostedApp),
        }
      }
      AppAction::Result(result) => {
        debug!("run_root_menu: unhandled child result {result:?} at the root");
      }
      AppAction::LaunchWasm(name) => {
        debug!("run_root_menu: launching wasm {name}");
        runner_ctx
          .host_ipc_sender
          .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartWasm(name))))
          .await;
        stack.push(AppStackEntry::HostedApp);
      }
      AppAction::LaunchNative(name) => {
        debug!("run_root_menu: launching native {name}");
        runner_ctx
          .host_ipc_sender
          .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartNative(name))))
          .await;
        stack.push(AppStackEntry::HostedApp);
      }
      // `Stop` (boot button at root) and `Continue` (PowerOff on a no-op
      // power manager, Back): re-enter the root menu, selection preserved.
      AppAction::Stop | AppAction::Continue => {}
    },
  }
}

/// Run a menu-app entry: render on (re-)entry, drive the app's loop, and
/// translate the result into a stack mutation.
async fn run_menu_app<P: Platform>(
  stack: &mut [AppStackEntry<P>],
  runner_ctx: &MenuRunnerContext<P>,
  display: &DisplayHandle,
) -> MenuAppOutcome<P> {
  let idx = stack.len() - 1;
  let AppStackEntry::MenuApp { app, pending_result } = &mut stack[idx] else {
    unreachable!("run_menu_app only dispatches on MenuApp entries");
  };
  // Clone the channel out of the entry borrow: it is the channel the menu
  // created when this app pushed a sub-app with `AppAction::Push`, and on
  // (re-)entry the child's result (if any) is buffered in it, delivered
  // through `AppRunContext` as `AppRunEvent::Result`.
  let pending_result = pending_result.clone();

  // (Re-)entry: show the app's current screen. Subsumes the old `on_shown` +
  // render — apps do their "refresh on show" work at the top of `run()`.
  let _ = display.signal(app.render());

  loop {
    let ctx = AppRunContext::new(&runner_ctx.platform, pending_result.as_ref());
    let action = select(app.run(ctx), runner_ctx.stack_event_handle.receive()).await;

    match action {
      Either::Second(StackEvent::Pushed(_)) => {
        // A hosted app was pushed on top (e.g. HTTP upload): hide this app.
        // It stays on the stack and resumes when the hosted app pops.
        return MenuAppOutcome::Hosted;
      }
      Either::Second(StackEvent::Popped) => {
        // Defensive stale pop: re-enter the app (it refreshes itself).
        let _ = display.signal(app.render());
      }
      Either::First(action) => match action {
        AppAction::Continue => {
          let _ = display.signal(app.render());
        }
        AppAction::Stop => {
          app.on_stop().await;
          debug!("run_menu_app: popped app");
          return MenuAppOutcome::Popped { result: None };
        }
        AppAction::Result(result) => {
          app.on_stop().await;
          debug!("run_menu_app: sub-app returned {result:?}");
          return MenuAppOutcome::Popped { result: Some(result) };
        }
        AppAction::LaunchWasm(name) => {
          debug!("run_menu_app: launching wasm {name}");
          runner_ctx
            .host_ipc_sender
            .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartWasm(name))))
            .await;
          return MenuAppOutcome::Hosted;
        }
        AppAction::LaunchNative(name) => {
          debug!("run_menu_app: launching native {name}");
          runner_ctx
            .host_ipc_sender
            .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartNative(name))))
            .await;
          return MenuAppOutcome::Hosted;
        }
        AppAction::LoadMenuApp(name) => {
          let mctx = MenuAppContext::new(runner_ctx.platform.clone(), runner_ctx.host_ipc_sender);
          match MenuAppType::<P>::load_app_async(&name, mctx, AppParams::None) {
            Ok(app) => return MenuAppOutcome::Loaded(Box::new(app), None),
            Err(mctx) => {
              if let Some(loader) = runner_ctx.app_loader {
                loader(name, mctx).await;
              }
              return MenuAppOutcome::Hosted;
            }
          }
        }
        AppAction::Push(name, params) => {
          debug!("run_menu_app: pushing sub-app {name} ({params:?})");
          let mctx = MenuAppContext::new(runner_ctx.platform.clone(), runner_ctx.host_ipc_sender);
          match MenuAppType::<P>::load_app_async(&name, mctx, params) {
            // The menu attaches the result channel to this entry (the
            // launcher) and delivers the child's result on pop.
            Ok(app) => return MenuAppOutcome::Loaded(Box::new(app), Some(ResultChannel::new(Channel::new()))),
            Err(mctx) => {
              warn!("run_menu_app: Push of unknown built-in {name}; launching hosted (no result)");
              if let Some(loader) = runner_ctx.app_loader {
                loader(name, mctx).await;
              }
              return MenuAppOutcome::Hosted;
            }
          }
        }
      },
    }
  }
}

/// Pump a hosted (WASM/native) app: forward input to the runtime and wait for
/// the pop. Buttons are `try_send`-ed (non-blocking) so a busy guest can
/// never stall the menu.
async fn run_hosted_app<P: Platform>(stack: &mut Vec<AppStackEntry<P>>, runner_ctx: &MenuRunnerContext<P>) {
  loop {
    let ctx = AppRunContext::new(&runner_ctx.platform, None);
    // `next_raw`: forward platform edges to the guest 1:1 — app-layer button
    // repeat is a built-in-app policy; a guest that wants it can apply
    // `ButtonRepeater` to the forwarded `HexButton`s itself.
    let event = select(ctx.next_raw(), runner_ctx.stack_event_handle.receive()).await;

    match event {
      Either::Second(StackEvent::Popped) => {
        debug!("run_hosted_app: popped hosted app");
        if stack.len() > 1 {
          let _ = stack.pop();
        }
        return;
      }
      Either::Second(StackEvent::Pushed(_)) => {
        // Another hosted app was launched from outside (HTTP upload) while
        // this one is still running. Push it; keep pumping the current entry
        // until it finishes, then the main loop dispatches a new pump for the
        // new top.
        stack.push(AppStackEntry::HostedApp);
      }
      Either::First(event) => match event {
        AppRunEvent::Input(AppInput::System(_)) => {
          debug!("run_hosted_app: boot button — stopping hosted app");
          runner_ctx
            .host_ipc_sender
            .try_send((0, HostIpcMessage::Runtime(HostRuntimeCommand::Stop)))
            .ok();
        }
        AppRunEvent::Input(AppInput::Button(hex)) => {
          runner_ctx
            .host_ipc_sender
            .try_send((0, HostIpcMessage::Wire(WireHostIpcMessage::HexButton(hex))))
            .ok();
        }
        AppRunEvent::Result(result) => {
          debug!("run_hosted_app: unhandled child result {result:?} (no parent to deliver to)");
        }
        AppRunEvent::Event(ev) => {
          debug!("run_hosted_app: event {ev:?}");
          // Forward navigation keys as HexButton presses to the guest — the
          // single nav-injection site for hosted apps (see
          // `nav_button_from_device_event`).
          if let Some(button) = nav_button_from_device_event(&ev) {
            runner_ctx
              .host_ipc_sender
              .try_send((0, HostIpcMessage::Wire(WireHostIpcMessage::HexButton(button))))
              .ok();
          }
        }
      },
    }
  }
}
