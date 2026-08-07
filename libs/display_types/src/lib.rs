#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LcdScreen {
  Blank,
  Splash,
  Headline(Icon40, String),
  Progress(String),
  BoundedProgress(u32, u32),
  Menu {
    menu: Vec<MenuLine>,
    selected: u32,
    /// Slide-in animation used when this menu is displayed/updated.
    #[serde(default)]
    animation: MenuAnimation,
  },
  /// Multi-line text buffer drawn inside a centred bordered frame. `lines`
  /// holds up to 8 lines; the line carrying `Some(cursor)` is the active
  /// (selected) line — the renderer centres its cursor, shifting the line so
  /// off-screen content is visible, and draws a blinking cursor on it.
  TextBuffer {
    lines: Vec<TextBufferLine>,
  },
  Notification(Icon40, String),
}

/// Timing of the `LcdScreen::Notification` overlay, in ms. Single source of
/// truth for the notification card: the renderer (`display_renderer`) derives
/// its animation timing from these, and the app layer (`ctx.notify`) uses the
/// total as the toast duration.
pub const NOTIFICATION_SLIDE_IN_MS: u64 = 350;
pub const NOTIFICATION_HOLD_MS: u64 = 2_000;
pub const NOTIFICATION_SLIDE_OUT_MS: u64 = 350;
pub const NOTIFICATION_TOTAL_MS: u64 = NOTIFICATION_SLIDE_IN_MS + NOTIFICATION_HOLD_MS + NOTIFICATION_SLIDE_OUT_MS;

/// Slide-in animation for a `LcdScreen::Menu`.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum MenuAnimation {
  /// Slide in from the left edge (content moves rightward).
  FromLeft,
  /// Slide in from the right edge (content moves leftward).
  FromRight,
  /// No animation — instant update.
  None,
}

impl Default for MenuAnimation {
  fn default() -> Self {
    Self::FromRight
  }
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct MenuLine(pub Icon20, pub String);

/// One line of a `LcdScreen::TextBuffer` frame.
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct TextBufferLine {
  pub text: String,
  /// Byte index of the blinking cursor within `text`. Lines with a cursor are
  /// the active (selected) line and are rendered with a highlight plus a
  /// blinking cursor that centres itself by shifting the line.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cursor: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Icon20 {
  Home,
  Config,
  Wifi,
  File,
  Info,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Icon40 {
  Info,
  Warn,
  Error,
  Wifi,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Image {
  RustLogo,
}

// ================================ LED ================================

pub const NUM_LEDS: usize = 12;

#[derive(Debug, Clone, Copy)]
pub enum LedRequest {
  Off,
  Solid(LedState),
  Rainbow,
  Breathe(LedState),
  Chase(LedState),
  Sparkle(LedState),
  TheaterChase(LedState),
  Fire,
}

#[derive(Debug, Clone, Copy)]
pub struct LedState {
  pub r: u8,
  pub g: u8,
  pub b: u8,
}

impl LedState {
  pub const fn new(r: u8, g: u8, b: u8) -> Self {
    Self { r, g, b }
  }
}
