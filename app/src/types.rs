// Re-export common display types
pub use display_types::{Icon20, Icon40, Image, LcdScreen, MenuLine};

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

// ================================ Device ================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SystemMessage {
  BootButton,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
  pub owner_name: String,
  pub app_store_url: String,
  pub firmware_url: String,
  pub wifi_mode: WifiMode,
  pub ap_ssid: String,
  #[serde(default)]
  pub ap_password: String,
  pub known_wifi_networks: Vec<KnownWifiNetwork>,
}

impl Default for DeviceConfig {
  fn default() -> Self {
    Self {
      owner_name: "Rustacean".to_string(),
      app_store_url: "http://apps.rustagon.chrisdell.info".to_string(),
      firmware_url: "http://firmware.rustagon.chrisdell.info".to_string(),
      wifi_mode: WifiMode::AccessPoint,
      ap_ssid: "Rustagon".to_string(),
      ap_password: "rustagon".to_string(),
      known_wifi_networks: Vec::new(),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WifiMode {
  Station,
  AccessPoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownWifiNetwork {
  pub ssid: String,
  pub pass: String,
}

// ================================ HTTP Status ================================

use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  channel::{Receiver, Sender},
};

pub type HttpChannel = embassy_sync::channel::Channel<CriticalSectionRawMutex, HttpStatusMessage, 1>;
pub type HttpSender = Sender<'static, CriticalSectionRawMutex, HttpStatusMessage, 1>;
pub type HttpReceiver = Receiver<'static, CriticalSectionRawMutex, HttpStatusMessage, 1>;

#[derive(Debug, Clone)]
pub enum HttpStatusMessage {
  Idle,
  Progress(u32, u32),
  ReceivedFile(Vec<u8>),
}

// ================================ WiFi ================================

#[derive(Debug, Clone, PartialEq)]
pub enum WifiDesiredState {
  Online,
  Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiResult {
  pub ssid: String,
  pub signal_strength: i8,
  pub password_required: bool,
}

// ================================ LED ================================

pub const NUM_LEDS: usize = 12;

#[derive(Debug, Clone)]
pub struct LedStates {
  pub leds: [LedState; NUM_LEDS],
}

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

// ================================ Input ================================

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HexButton {
  Up,
  Right,
  Fire,
  Down,
  Left,
  HexA,
  HexB,
  HexC,
  HexD,
  HexE,
  HexF,
  Touch01,
  Touch02,
  Touch03,
  Touch04,
  Touch05,
  Touch06,
  Touch07,
  Touch08,
  Touch09,
  Touch10,
  Touch11,
  Touch12,
}

// ================================ WebSocket ================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebSocketIncomingMessage {
  HexButton(HexButton),
  SystemMessage(SystemMessage),
}

pub type WebSocketIncomingChannel = embassy_sync::channel::Channel<CriticalSectionRawMutex, WebSocketIncomingMessage, 1>;
pub type WebSocketIncomingSender = Sender<'static, CriticalSectionRawMutex, WebSocketIncomingMessage, 1>;
pub type WebSocketIncomingReceiver = Receiver<'static, CriticalSectionRawMutex, WebSocketIncomingMessage, 1>;

// ================================ OTA ================================

#[derive(Debug, Clone)]
pub enum OtaError {
  FlashWrite,
  FlashRead,
  InvalidSlot,
  NotSupported,
}
