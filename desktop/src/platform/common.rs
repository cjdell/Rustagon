use app::platform::hexpansion::HexpansionManager;
use app::platform::led::{LedError, LedManager};
use app::platform::power::{PowerManager, PowerStatus};
use app::platform::system::SystemManager;
use app::platform::wifi::{WiFiManager, WifiStatus};
use app::types::{DeviceEvent, HexpansionEvent, HexpansionInfo, LedRequest, SystemMessage, WifiDesiredState, WifiResult};
use app::utils::{EventQueue, WatchedValue};
use std::pin::Pin;

#[derive(Debug)]
pub struct DesktopLedManager;
impl LedManager for DesktopLedManager {
  fn request(&self, _: LedRequest) -> Result<(), LedError> {
    Ok(())
  }
}

#[derive(Debug)]
pub struct DesktopPowerManager;
impl PowerManager for DesktopPowerManager {
  fn power_off(&self) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async {
      std::process::exit(0);
    })
  }
  fn get_status(&self) -> Pin<Box<dyn std::future::Future<Output = PowerStatus> + Send + '_>> {
    Box::pin(async {
      PowerStatus {
        vbat_mv: 3700,
        vsys_mv: 3700,
        vbus_mv: 0,
        charge_current_ma: 0,
        charge_voltage_mv: 4200,
        input_current_limit_ma: 500,
        is_charging: false,
        is_power_present: false,
        battery_fault: false,
      }
    })
  }
  fn wait_for_change(&self) -> Pin<Box<dyn std::future::Future<Output = PowerStatus> + Send + '_>> {
    // The desktop power status is a fixed stub and never changes.
    Box::pin(std::future::pending())
  }
}

/// Same structure as the firmware's `HardwareWifiManager`: a single
/// `WatchedValue<WifiStatus>` as the source of truth. Block 10 will add a
/// simulated connection task that updates it; for now it stays `Offline`.
#[derive(Clone, Debug)]
pub struct DesktopWifiManager {
  status: WatchedValue<WifiStatus>,
}

impl DesktopWifiManager {
  pub fn new() -> Self {
    Self {
      status: WatchedValue::new(WifiStatus::Offline),
    }
  }
}

impl Default for DesktopWifiManager {
  fn default() -> Self {
    Self::new()
  }
}

impl WiFiManager for DesktopWifiManager {
  fn get_status(&self) -> Pin<Box<dyn std::future::Future<Output = WifiStatus> + Send + '_>> {
    Box::pin(self.status.get())
  }
  fn wait_for_status_change(&self) -> Pin<Box<dyn std::future::Future<Output = WifiStatus> + Send + '_>> {
    self.status.wait_for_change()
  }
  fn set_desired_state(&self, _: WifiDesiredState) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async {})
  }
  fn scan(&self) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<WifiResult>, ()>> + Send + '_>> {
    Box::pin(async { Ok(Vec::new()) })
  }
}

/// Same depth as the firmware's system event queue.
const SYSTEM_EVENT_QUEUE_DEPTH: usize = 8;

type SystemEventQueue = EventQueue<SystemMessage, SYSTEM_EVENT_QUEUE_DEPTH>;

#[derive(Clone, Debug)]
pub struct DesktopSystemManager {
  events: SystemEventQueue,
}

impl DesktopSystemManager {
  pub fn new() -> Self {
    Self {
      events: SystemEventQueue::new(),
    }
  }

  /// Called from the minifb thread when the boot button equivalent is pressed.
  /// Clones of the manager share the queue, so the minifb thread keeps a clone.
  pub fn push_message(&self, msg: SystemMessage) {
    // Drop on overflow, same as the firmware button task's `try_push`.
    let _ = self.events.try_push(msg);
  }
}

impl SystemManager for DesktopSystemManager {
  fn next_button(&self) -> Pin<Box<dyn std::future::Future<Output = SystemMessage> + Send + '_>> {
    Box::pin(self.events.next())
  }
  fn inject(&self, msg: SystemMessage) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(self.events.push(msg))
  }
}

/// Same depth as the firmware's hexpansion event queue.
const HEXPANSION_EVENT_QUEUE_DEPTH: usize = 16;

/// Same depth as the firmware's `DeviceEventQueue`.
const DEVICE_EVENT_QUEUE_DEPTH: usize = 32;

type HexpansionEventQueue = EventQueue<HexpansionEvent, HEXPANSION_EVENT_QUEUE_DEPTH>;
type DeviceEventQueue = EventQueue<DeviceEvent, DEVICE_EVENT_QUEUE_DEPTH>;

/// Mirrors `HardwareHexpansionManager`: the manager owns both event queues
/// rather than relying on module-level statics.
#[derive(Clone, Debug)]
pub struct DesktopHexpansionManager {
  events: HexpansionEventQueue,
  device_events: DeviceEventQueue,
}

impl DesktopHexpansionManager {
  pub fn new() -> Self {
    Self {
      events: HexpansionEventQueue::new(),
      device_events: DeviceEventQueue::new(),
    }
  }

  /// Injection point for hexpansion plug/unplug events. Block 10 will feed this
  /// from a file-backed simulation; until then it stays empty, so `next_event`
  /// blocks as before.
  #[allow(dead_code)]
  pub fn push_event(&self, event: HexpansionEvent) {
    let _ = self.events.try_push(event);
  }

  /// Called from the minifb thread when a keyboard hexpansion key is pressed/released.
  /// Clones of the manager share the queue, so the minifb thread keeps a clone.
  pub fn push_device_event(&self, event: DeviceEvent) {
    // Drop on overflow, same as the firmware driver's `try_push`.
    let _ = self.device_events.try_push(event);
  }
}

impl HexpansionManager for DesktopHexpansionManager {
  fn next_event(&self) -> Pin<Box<dyn std::future::Future<Output = HexpansionEvent> + Send + '_>> {
    Box::pin(self.events.next())
  }

  fn try_next_event(&self) -> Option<HexpansionEvent> {
    self.events.try_next()
  }

  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)> {
    Vec::new()
  }

  fn next_device_event(&self) -> Pin<Box<dyn std::future::Future<Output = DeviceEvent> + Send + '_>> {
    Box::pin(self.device_events.next())
  }

  fn try_next_device_event(&self) -> Option<DeviceEvent> {
    self.device_events.try_next()
  }
}
