//! The `MenuApp` contract: how an app is driven by the menu and what lifecycle
//! hooks it gets.
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

use crate::platform::{display::DisplayHandle, Platform};
use crate::protocol::HostIpcSender;
use crate::types::{DeviceEvent, HexButton, HexpansionEvent, Icon40};
use crate::utils::sleep;
use alloc::string::{String, ToString};
use core::fmt;
use display_types::LcdScreen;

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
  /// This awaits for the toast duration so the menu loop doesn't re-render the
  /// app's screen over the notification before it has been seen. Callers can
  /// restore their own screen afterwards with `update_lcd(self.render())` if
  /// needed.
  pub async fn notify(&self, msg: impl Into<String>, icon: Icon40) {
    let _ = self.display.signal(LcdScreen::Notification(icon, msg.into()));
    sleep(NOTIFICATION_MS).await;
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

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
  Continue,
  Stop,
  LaunchWasm(String),
  LaunchNative(String),
}

pub trait MenuApp {
  fn render(&self) -> LcdScreen;
  async fn init(&mut self);
  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction;

  /// Called when external events occur (hexpansion plug/unplug, etc.) while
  /// the app is in the foreground. Default does nothing — override to react
  /// to state changes without requiring a button press.
  async fn handle_event(&mut self, _event: AppEvent) {}

  /// Called by the menu on a fixed cadence (~[`crate::menu::APP_TICK_MS`]) and
  /// after every external event, so the app can drain background channels
  /// (TCP, HTTP) and do periodic work without waiting for input. Default does
  /// nothing.
  async fn tick(&mut self) {}

  /// Called just before the app is popped off the stack (boot button, `Stop`
  /// action, or a sub-app popping). The default forwards `Stop` to the input
  /// path so apps get their usual cleanup; override for custom teardown (e.g.
  /// SSH closing its socket on *every* pop, not just when it sees `Stop`).
  async fn on_stop(&mut self) {
    let _ = self.handle_input(MenuAppInput::Stop).await;
  }

  /// Called when the app becomes visible again — on first launch and whenever
  /// it is revealed after a sub-app pops. Default does nothing — override to
  /// refresh state on return (e.g. re-scan files).
  async fn on_shown(&mut self) {}
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
  Hexpansion(HexpansionEvent),
  Device(DeviceEvent),
}

pub enum MenuAppInput {
  Button(HexButton),
  Stop,
}
