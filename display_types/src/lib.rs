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
  Menu { menu: Vec<MenuLine>, selected: u32 },
  Notification(Icon40, String),
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
