// Re-export app types (platform-agnostic domain types)
pub use app::types::*;

// Firmware-specific channel type aliases
use crate::protocol::{HostIpcMessage, WasmIpcMessage};
use crate::utils::spi::SpiExclusiveDevice;
use display_interface_spi::SPIInterface;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_sync::pubsub::{PubSubChannel, Publisher, Subscriber};
use embassy_sync::watch;
use esp_hal::gpio::Output;

pub type DisplayInterface<'a> = SPIInterface<SpiExclusiveDevice<'a>, Output<'a>>;

pub type SystemWatch = watch::Watch<CriticalSectionRawMutex, SystemMessage, 1>;
pub type SystemSender = watch::Sender<'static, CriticalSectionRawMutex, SystemMessage, 1>;
pub type SystemReceiver = watch::Receiver<'static, CriticalSectionRawMutex, SystemMessage, 1>;

pub type WifiScanPubSub = PubSubChannel<CriticalSectionRawMutex, WifiResult, 8, 4, 4>;
pub type WifiScanPublisher = Publisher<'static, CriticalSectionRawMutex, WifiResult, 8, 4, 4>;
pub type WifiScanSubscriber = Subscriber<'static, CriticalSectionRawMutex, WifiResult, 8, 4, 4>;

pub type WifiStatusWatch = watch::Watch<CriticalSectionRawMutex, app::platform::WifiStatus, 1>;
pub type WifiStatusWatchSender = watch::Sender<'static, CriticalSectionRawMutex, app::platform::WifiStatus, 1>;
pub type WifiStatusWatchReceiver = watch::Receiver<'static, CriticalSectionRawMutex, app::platform::WifiStatus, 1>;

pub type WasmIpcChannel = Channel<CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type WasmIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type WasmIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, WasmIpcMessage), 1>;
pub type HostIpcChannel = Channel<CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcSender = Sender<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;
pub type HostIpcReceiver = Receiver<'static, CriticalSectionRawMutex, (u32, HostIpcMessage), 1>;

#[derive(Clone)]
pub enum I2cMessage {
  HexButton(HexButton),
}

pub type HexButtonChannel = PubSubChannel<CriticalSectionRawMutex, I2cMessage, 10, 2, 2>;
pub type HexButtonSender = Publisher<'static, CriticalSectionRawMutex, I2cMessage, 10, 2, 2>;
pub type HexButtonReceiver = Subscriber<'static, CriticalSectionRawMutex, I2cMessage, 10, 2, 2>;

pub type ButtonEventChannel = Channel<CriticalSectionRawMutex, HexButton, 10>;
