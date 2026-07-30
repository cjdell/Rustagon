pub mod tca8418;

use app::platform::hexpansion::DeviceIo;
use app::types::DeviceEvent;
use core::pin::Pin;
use embassy_executor::Spawner;

use crate::utils::EventQueue;

/// Capacity of the shared device event queue.
pub const DEVICE_EVENT_QUEUE_DEPTH: usize = 32;

/// Cloneable, shared queue for device driver events.
pub type DeviceEventQueue = EventQueue<DeviceEvent, DEVICE_EVENT_QUEUE_DEPTH>;

/// A driver factory creates the driver for a detected hexpansion.
///
/// The factory is called with the hexpansion's `DeviceIo`, a `DeviceEventQueue`
/// that the driver pushes events into, and a `Spawner` for any subtasks.
pub type DriverFactory = fn(DeviceIo, DeviceEventQueue, Spawner);

/// A single entry in the driver registry table.
pub struct DriverEntry {
  pub vid: u16,
  pub pid: u16,
  pub factory: DriverFactory,
}
