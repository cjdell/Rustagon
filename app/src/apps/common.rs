//! The `MenuApp` contract: apps own their event loop.
//!
//! # The run-loop model
//!
//! The menu launches an app once and the app drives itself: `run()` is an
//! async loop that multiplexes user input, external events, timers and
//! background work through [`AppRunContext`]. The app returns when the user
//! has quit (`AppAction::Stop`) or wants to launch something
//! (`LaunchWasm`/`LaunchNative`/`LoadMenuApp`). While an app is foregrounded
//! the menu only watches the stack signal (sub-app pushes/pops from the
//! hosted-app runtime); everything else flows through `AppRunContext`.
//!
//! This is what lets push-driven apps (SSH, future chat/sensors) keep running
//! while they await a connection or a download: the boot button, keyboard
//! events and timers are just more branches of the same `select`, so a long
//! operation can be cancelled mid-flight and inbound data renders without
//! waiting for input.
//!
//! # Logging convention
//!
//! Apps tag their log lines with their name (e.g. `SshApp:`) and use levels by
//! cost and noise:
//! - `info!` — state transitions, lifecycle milestones, user-visible events
//!   (connection opened, key loaded, download complete).
//! - `debug!` — per-frame / per-packet chatter (handshake events, decoded
//!   packets, button key handling). Filtered at runtime via `ESP_LOG`.
//! - `error!`/`warn!` — recoverable failures the user may see.

use crate::kv::KvNamespace;
use crate::platform::{display::DisplayHandle, Platform};
use crate::protocol::HostIpcSender;
use crate::types::{DeviceEvent, HexButton, HexpansionEvent, Icon40};
use crate::utils::sleep;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use core::fmt;
use display_types::LcdScreen;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

/// How long a `ctx.notify` toast stays up. Sourced from `display_types` (the
/// single source of truth for the `LcdScreen::Notification` timing, also used
/// by `display_renderer` to draw the card), so the toast lifetime always
/// matches the rendering code — no duplicated magic number.
pub use display_types::NOTIFICATION_TOTAL_MS as NOTIFICATION_MS;

pub trait AppName {
  fn app_name() -> &'static str;
}

/// A common error type for fallible app operations. Apps return
/// `Result<_, AppError>` from ops and surface the message via
/// [`AppError::to_display`] (or `ctx.notify`). Replace per-app error plumbing
/// (SSH's `fail()`, OTA's error screen) with this.
#[derive(Debug, Clone, PartialEq)]
pub enum AppError {
  /// A plain message to show the user.
  Message(String),
  /// A network (TCP/HTTP) operation failed.
  Network,
  /// A filesystem/storage operation failed.
  Storage,
  /// An operation timed out.
  Timeout,
  /// A required resource (file, app, network) was not found.
  NotFound(String),
  /// The platform/host does not support the requested operation.
  Unsupported(String),
}

impl AppError {
  /// The user-facing message for this error.
  pub fn to_display(&self) -> &str {
    match self {
      AppError::Message(msg) => msg,
      AppError::Network => "Network error",
      AppError::Storage => "Storage error",
      AppError::Timeout => "Timed out",
      AppError::NotFound(msg) => msg,
      AppError::Unsupported(msg) => msg,
    }
  }
}

impl fmt::Display for AppError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.to_display())
  }
}

impl From<&str> for AppError {
  fn from(msg: &str) -> Self {
    AppError::Message(msg.to_string())
  }
}

impl From<String> for AppError {
  fn from(msg: String) -> Self {
    AppError::Message(msg)
  }
}

/// Long-lived dependencies an app keeps for its whole lifetime. Constructed
/// by the menu (or the app loader) once at launch, owned by the app.
pub struct MenuAppContext<P: Platform> {
  pub platform: P,
  pub display: DisplayHandle,
  pub host_ipc_sender: HostIpcSender,
}

impl<P: Platform> MenuAppContext<P> {
  pub fn new(platform: P, host_ipc_sender: HostIpcSender) -> Self {
    let display = platform.display_manager();
    Self {
      platform,
      display,
      host_ipc_sender,
    }
  }

  pub fn update_lcd(&self, screen: LcdScreen) {
    let _ = self.display.signal(screen);
  }

  /// Render a transient toast on the LCD using the renderer's shared
  /// notification overlay (`LcdScreen::Notification`): a card that slides in,
  /// holds ~2 s, slides out, and auto-restores the previous screen.
  ///
  /// This awaits for the toast duration so the caller can pause its loop
  /// until the toast has been seen. Callers can restore their own screen
  /// afterwards with `update_lcd(self.render())` if needed.
  pub async fn notify(&self, msg: impl Into<String>, icon: Icon40) {
    let _ = self.display.signal(LcdScreen::Notification(icon, msg.into()));
    sleep(NOTIFICATION_MS).await;
  }

  /// A per-app key-value store namespace for this app's settings and state
  /// (one JSON file per namespace under `/apps/<name>/` on the platform
  /// storage). See the [`crate::kv`] module docs.
  pub fn kv(&self, namespace: &str) -> KvNamespace {
    KvNamespace::new(self.platform.storage_manager(), namespace)
  }
}

impl<P: Platform> Clone for MenuAppContext<P> {
  fn clone(&self) -> Self {
    Self {
      platform: self.platform.clone(),
      display: self.display.clone(),
      host_ipc_sender: self.host_ipc_sender,
    }
  }
}

/// A small typed payload handed to a sub-app when it is pushed with
/// [`AppAction::Push`]. Kept intentionally tiny: new kinds of sub-app
/// interaction add a variant here (and a matching [`AppResult`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppParams {
  /// No parameters.
  None,
  /// Run the Files app in picker mode: Fire on a file returns
  /// [`AppResult::Path`], back/boot returns `Cancelled`.
  PickFile { message: String },
  /// Run the confirmation dialog.
  Confirm { title: String, message: String },
}

/// The value a sub-app hands back to the app that pushed it. Delivered to
/// the parent as [`AppRunEvent::Result`] on the parent's re-entry (see
/// [`AppRunContext::next`]).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AppResult {
  /// A file path chosen in a picker.
  Path(String),
  /// The answer to a confirmation dialog.
  Confirm(bool),
  /// The sub-app was dismissed without a value (boot button, back).
  Cancelled,
}

/// What the menu did with an app's run-loop result.
#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
  /// Keep the app in the foreground. The menu re-enters `run()` — so a
  /// `Continue` should be reserved for "one iteration handled, carry on"
  /// return paths, not the hot loop (loop inside `run()` instead).
  Continue,
  /// The user has quit the app (boot button, an explicit back action). The
  /// menu calls `on_stop` and pops the app. For a sub-app pushed with
  /// [`AppAction::Push`], the parent receives [`AppResult::Cancelled`].
  Stop,
  /// Launch a WASM app by name (resolved by the host's app store).
  LaunchWasm(String),
  /// Launch a native app by name.
  LaunchNative(String),
  /// Launch a built-in menu app by name (used by the root menu).
  LoadMenuApp(String),
  /// Push a built-in sub-app (by name, resolved like [`AppAction::LoadMenuApp`])
  /// and await its result. The menu re-enters this app's `run()` once the
  /// sub-app returns; the result arrives as [`AppRunEvent::Result`] — a child
  /// returning `Stop` without a result delivers [`AppResult::Cancelled`].
  Push(String, AppParams),
  /// The terminal action for a sub-app: hand `result` back to the app that
  /// pushed this one.
  Result(AppResult),
}

/// The per-push result channel the menu creates for a
/// [`AppAction::Push`]: capacity 1, so the child's result is buffered until
/// the parent's `run()` re-enters and drains it. `Arc`-wrapped because
/// `embassy_sync::channel::Channel` is not `Clone` in embassy-sync 0.8 (all
/// its methods take `&self`).
pub type ResultChannel = Arc<Channel<CriticalSectionRawMutex, AppResult, 1>>;

/// One input event for an app's run loop.
#[derive(Debug, Clone)]
pub enum AppInput {
  /// A hex (badge) button press or release. Keyboard navigation keys
  /// (arrows, Enter) arrive here as `HexButton`s — the platform surfaces them
  /// identically to the physical badge buttons.
  Button(HexButton),
  /// The system/boot button ("BOOP") was pressed. The convention is to quit:
  /// return [`AppAction::Stop`] (see [`AppRunEvent::exit_action`]).
  System(crate::types::SystemMessage),
}

/// One external (non-input) event for an app's run loop.
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
  Hexpansion(HexpansionEvent),
  Device(DeviceEvent),
}

/// A single multiplexed event from an app's run loop.
#[derive(Debug, Clone)]
pub enum AppRunEvent {
  Input(AppInput),
  Event(AppEvent),
  /// A sub-app this app pushed returned a result (see [`AppAction::Push`]).
  /// Only possible after the menu has re-entered `run()` following a `Push`.
  Result(AppResult),
}

impl AppRunEvent {
  /// The run-loop exit this event implies, if any:
  ///
  /// - boot button → [`AppAction::Stop`] (the badge's "home" button always
  ///   leaves the current app);
  ///
  /// Returns `None` for everything else — hex buttons, keyboard/hexpansion
  /// events — which the app dispatches on itself.
  pub fn exit_action(&self) -> Option<AppAction> {
    match self {
      AppRunEvent::Input(AppInput::System(_)) => Some(AppAction::Stop),
      _ => None,
    }
  }
}

/// The per-invocation event multiplexer the menu hands to
/// [`MenuApp::run`]. Constructed per `run()` entry; borrows the runner's
/// platform, so it lives only for the duration of `run()`.
///
/// Apps that need a longer-lived copy of a platform dependency should keep it
/// from their [`MenuAppContext`] — this type exists so `run()` can multiplex
/// without threading channels around.
///
/// Note the stack signal is deliberately *not* part of this multiplexer: the
/// menu watches it (sub-app push/pop) around `run()` and re-enters the app,
/// so apps never have to reason about their own hiding/showing.
pub struct AppRunContext<'a, P: Platform> {
  platform: &'a P,
  /// The pending sub-app result channel the menu created for a
  /// [`AppAction::Push`], if the app is about to receive one. The result
  /// (already buffered by the menu) is delivered as
  /// [`AppRunEvent::Result`] by [`AppRunContext::next`].
  result: Option<&'a ResultChannel>,
}

impl<'a, P: Platform> AppRunContext<'a, P> {
  pub fn new(platform: &'a P, result: Option<&'a ResultChannel>) -> Self {
    Self { platform, result }
  }

  /// Non-blocking drain of a pending sub-app result, if the menu has already
  /// delivered one. Apps that multiplex `next_input()`/`next_event()` directly
  /// (instead of `next()`) should call this on (re-)entry after a `Push`.
  pub fn try_next_result(&self) -> Option<AppResult> {
    self.result.and_then(|channel| channel.try_receive().ok())
  }

  /// Wait for the pending sub-app result. Only meaningful when the menu
  /// created a result channel for this re-entry (i.e. the app previously
  /// returned [`AppAction::Push`]); panics otherwise — use
  /// [`AppRunContext::next`], which includes the result branch when present.
  pub async fn next_result(&self) -> AppResult {
    let channel = self.result.expect("next_result requires a pending Push result");
    channel.receive().await
  }

  /// A clone of the platform handle (storage, wifi, config, http, tcp,
  /// spawner, ...). Apps normally hold their own copy in [`MenuAppContext`].
  pub fn platform(&self) -> P {
    self.platform.clone()
  }

  /// Wait for the next user input: a hex button or the boot button.
  pub async fn next_input(&self) -> AppInput {
    let system_handle = self.platform.system_manager();
    let input_handle = self.platform.input_manager();
    let system = system_handle.next_button();
    let button = input_handle.next_button();
    match embassy_futures::select::select(system, button).await {
      embassy_futures::select::Either::First(msg) => AppInput::System(msg),
      embassy_futures::select::Either::Second(btn) => AppInput::Button(btn),
    }
  }

  /// Wait for the next external event (hexpansion plug/unplug, device/keyboard).
  pub async fn next_event(&self) -> AppEvent {
    let hexpansion_handle = self.platform.hexpansion_manager();
    let hexpansion = hexpansion_handle.next_event();
    let device = hexpansion_handle.next_device_event();
    match embassy_futures::select::select(hexpansion, device).await {
      embassy_futures::select::Either::First(ev) => AppEvent::Hexpansion(ev),
      embassy_futures::select::Either::Second(ev) => AppEvent::Device(ev),
    }
  }

  /// Wait for the next input *or* external event, flattened into one enum —
  /// the common case for apps. Apps that also want to multiplex their own
  /// work (a TCP socket, a timer, a download) `select!` over `next()` and
  /// those branches directly.
  ///
  /// When the menu has a pending result for this re-entry (after the app
  /// returned [`AppAction::Push`]), the result is an additional branch and
  /// arrives first as [`AppRunEvent::Result`] (the menu buffers it before the
  /// parent re-enters, so it never loses the race against a button press).
  pub async fn next(&self) -> AppRunEvent {
    match self.result {
      None => match embassy_futures::select::select(self.next_input(), self.next_event()).await {
        embassy_futures::select::Either::First(input) => AppRunEvent::Input(input),
        embassy_futures::select::Either::Second(event) => AppRunEvent::Event(event),
      },
      Some(channel) => {
        let io = embassy_futures::select::select(self.next_input(), self.next_event());
        match embassy_futures::select::select(io, channel.receive()).await {
          embassy_futures::select::Either::First(embassy_futures::select::Either::First(input)) => AppRunEvent::Input(input),
          embassy_futures::select::Either::First(embassy_futures::select::Either::Second(event)) => AppRunEvent::Event(event),
          embassy_futures::select::Either::Second(result) => AppRunEvent::Result(result),
        }
      }
    }
  }
}

/// A keyboard event's navigation `HexButton`, if the key is one of the
/// nav-only keys (Escape→HexF, Tab→HexE). Arrows/Enter never arrive as
/// keyboard events — the platform already surfaces them as `HexButton`
/// presses — so they have no entry here.
///
/// This is the *single* place nav-key injection happens. It is used by the
/// hosted-app pump and the root menu (both of which can only consume
/// `HexButton`s); built-in menu apps receive the raw keyboard event instead,
/// so a Tab in the Editor still types a tab.
pub fn nav_button_from_device_event(event: &AppEvent) -> Option<HexButton> {
  let AppEvent::Device(DeviceEvent::Keyboard(ke)) = event else {
    return None;
  };
  use crate::types::{KeyCode, KeyEventType};
  let base = match ke.code {
    KeyCode::Escape => HexButton::HexF,
    KeyCode::Tab => HexButton::HexE,
    _ => return None,
  };
  Some(match ke.typ {
    KeyEventType::Pressed => base,
    KeyEventType::Released => base.released(),
  })
}

/// An app that owns its event loop. The menu constructs the app, enters the
/// stack, and then calls `run()`; the app does everything until it returns.
pub trait MenuApp<P: Platform> {
  /// The screen the app would show right now. The menu signals it to the
  /// display on (re-)entry; the app signals updates itself via
  /// [`MenuAppContext::update_lcd`] while it runs.
  fn render(&self) -> LcdScreen;

  /// The app's main loop. Multiplexes input/events via `ctx.next()` (or
  /// `select!` over `ctx.next_input()`/`ctx.next_event()` plus the app's own
  /// work) and returns when the user quits or wants to launch something.
  ///
  /// Re-entry: the menu calls `run()` again whenever the app is revealed
  /// after a sub-app pops, or after a `Continue` return. Do any
  /// "refresh on show" work at the top of the loop body's first iteration
  /// (or keep it idempotent) — there is no separate `init`/`on_shown`.
  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction;

  /// Called just before the app is popped off the stack (boot button,
  /// `Stop` action, or a sub-app popping). Default does nothing — override
  /// for custom teardown (e.g. SSH closing its socket on *every* pop, not
  /// just when it saw its own `Stop`).
  async fn on_stop(&mut self) {}
}
