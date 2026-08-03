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
  Notification(Icon40, String),
}

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
