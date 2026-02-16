use crate::lib::protocol::{HostIpcMessage, WasmIpcMessage};
use crate::utils::led_service::LedState;
use crate::utils::spi::SpiExclusiveDevice;
use crate::utils::state::PersistentStateService;
use alloc::vec;
use alloc::{
  string::{String, ToString},
  vec::Vec,
};
use core::cell::RefCell;
use display_interface_spi::SPIInterface;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::pubsub::{PubSubChannel, Publisher, Subscriber};
use embassy_sync::watch;
use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  channel::{Channel, Receiver, Sender},
};
use esp_hal::Blocking;
use esp_hal::gpio::Output;
use esp_hal::i2c::master::I2c;
use serde::{Deserialize, Serialize};

// ================================ Device ================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemMessage {
  BootButton,
}

pub type SystemWatch = watch::Watch<CriticalSectionRawMutex, SystemMessage, 1>;

pub type SystemSender = watch::Sender<'static, CriticalSectionRawMutex, SystemMessage, 1>;
pub type SystemReceiver = watch::Receiver<'static, CriticalSectionRawMutex, SystemMessage, 1>;

// ================================ Device ================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)] // This will merge in defaults for new properties and effectively allow migrations
pub struct DeviceConfig {
  pub owner_name: String,
  pub app_store_url: String,
  pub firmware_url: String,
  pub wifi_mode: WifiMode,
  pub ap_ssid: String,
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
      known_wifi_networks: vec![],
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WifiMode {
  Station,
  AccessPoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownWifiNetwork {
  pub ssid: String,
  pub pass: String,
}

pub type DeviceState = PersistentStateService<DeviceConfig>;

// ================================ HTTP ================================

#[derive(Debug, Clone)]
pub enum HttpStatusMessage {
  None,
  Progress(u32, u32),
  ReceivedFile(Vec<u8>),
}

pub type HttpChannel = Channel<CriticalSectionRawMutex, HttpStatusMessage, 10>;

pub type HttpSender = Sender<'static, CriticalSectionRawMutex, HttpStatusMessage, 10>;
pub type HttpReceiver = Receiver<'static, CriticalSectionRawMutex, HttpStatusMessage, 10>;

// ================================ Wifi ================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiResult {
  pub ssid: String,
  pub signal_strength: i8,
  pub password_required: bool,
}

// ================================ LCD ================================

pub type DisplayInterface<'a> = SPIInterface<SpiExclusiveDevice<'a>, Output<'a>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LcdScreen {
  Blank,
  Splash,
  Headline(Icon40, String),
  Progress(String),
  BoundedProgress(u32, u32),
  Menu { menu: Vec<MenuLine>, selected: u32 },
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

// ================================ LED ================================

pub const NUM_LEDS: usize = 12; // 1 internal + 12 of front. 6 more on back disabled

#[derive(Debug, Clone)]
pub struct LedStates {
  pub leds: [LedState; NUM_LEDS],
}

impl LedState {
  pub fn new(r: u8, g: u8, b: u8) -> Self {
    Self { r, g, b }
  }
}

#[derive(Debug, Clone)]
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

pub type LedChannel = Channel<CriticalSectionRawMutex, LedRequest, 10>;

pub type LedSender = Sender<'static, CriticalSectionRawMutex, LedRequest, 10>;
pub type LedReceiver = Receiver<'static, CriticalSectionRawMutex, LedRequest, 10>;

// ================================ i2C ================================

pub type I2cMutux = Mutex<NoopRawMutex, RefCell<I2c<'static, Blocking>>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HexButton {
  A,
  B,
  C,
  D,
  E,
  F,
}

#[derive(Clone, Debug)]
pub enum I2cMessage {
  HexButton(HexButton),
  DisplayReset,
}

pub type HexButtonChannel = PubSubChannel<CriticalSectionRawMutex, I2cMessage, 10, 2, 2>;

pub type HexButtonSender = Publisher<'static, CriticalSectionRawMutex, I2cMessage, 10, 2, 2>;
pub type HexButtonReceiver = Subscriber<'static, CriticalSectionRawMutex, I2cMessage, 10, 2, 2>;

pub enum PowerCtrl {
  PowerOff,
}

pub type PowerCtrlChannel = Channel<CriticalSectionRawMutex, PowerCtrl, 1>;
pub type PowerCtrlSender = Sender<'static, CriticalSectionRawMutex, PowerCtrl, 1>;
pub type PowerCtrlReceiver = Receiver<'static, CriticalSectionRawMutex, PowerCtrl, 1>;

// ================================ WASM ================================

pub type WasmIpcChannel = Channel<CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type WasmIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type WasmIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;

pub type HostIpcChannel = Channel<CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;

// ================================ Web Socket ================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WebSocketIncomingMessage {
  HexButton(HexButton),
  SystemMessage(SystemMessage),
}

pub type WebSocketIncomingChannel = Channel<CriticalSectionRawMutex, WebSocketIncomingMessage, 1>;
pub type WebSocketIncomingSender = Sender<'static, CriticalSectionRawMutex, WebSocketIncomingMessage, 1>;
pub type WebSocketIncomingReceiver = Receiver<'static, CriticalSectionRawMutex, WebSocketIncomingMessage, 1>;
