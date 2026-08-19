pub mod tca8418;

pub use super::input::ButtonEventQueue;
use app::platform::hexpansion::DeviceIo;
use app::types::DeviceEvent;
use embassy_executor::Spawner;

use app::utils::EventQueue;

/// Capacity of the shared device event queue.
pub const DEVICE_EVENT_QUEUE_DEPTH: usize = 32;

/// Cloneable, shared queue for device driver events.
pub type DeviceEventQueue = EventQueue<DeviceEvent, DEVICE_EVENT_QUEUE_DEPTH>;

/// A driver factory creates the driver for a detected hexpansion.
///
/// The factory is called with the hexpansion's `DeviceIo`, a `DeviceEventQueue`
/// that the driver pushes events into, the shared button queue (for keyboard
/// hexpansions whose arrow/enter keys surface as `HexButton` presses), and a
/// `Spawner` for any subtasks.
pub type DriverFactory = fn(DeviceIo, DeviceEventQueue, ButtonEventQueue, Spawner);

/// A single entry in the driver registry table.
pub struct DriverEntry {
  pub vid: u16,
  pub pid: u16,
  pub factory: DriverFactory,
}
