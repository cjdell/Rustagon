use app::platform::hexpansion::HexpansionManager;
use app::platform::led::{LedError, LedManager};
use app::platform::power::{PowerManager, PowerStatus};
use app::platform::system::SystemManager;
use app::platform::wifi::{WiFiManager, WifiStatus};
use app::types::{DeviceEvent, HexpansionEvent, HexpansionInfo, LedRequest, SystemMessage, WifiDesiredState, WifiResult};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
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
}

#[derive(Debug)]
pub struct DesktopWifiManager;
impl WiFiManager for DesktopWifiManager {
  fn get_status(&self) -> Pin<Box<dyn std::future::Future<Output = WifiStatus> + Send + '_>> {
    Box::pin(async { WifiStatus::Offline })
  }
  fn wait_for_status_change(&self) -> Pin<Box<dyn std::future::Future<Output = WifiStatus> + Send + '_>> {
    Box::pin(async { std::future::pending::<WifiStatus>().await })
  }
  fn set_desired_state(&self, _: WifiDesiredState) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async {})
  }
  fn scan(&self) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<WifiResult>, ()>> + Send + '_>> {
    Box::pin(async { Ok(Vec::new()) })
  }
}

static SYSTEM_SIGNAL: Signal<CriticalSectionRawMutex, SystemMessage> = Signal::new();

#[derive(Debug)]
pub struct DesktopSystemManager;

impl DesktopSystemManager {
  pub fn push_message(msg: SystemMessage) {
    SYSTEM_SIGNAL.signal(msg);
  }
}

impl SystemManager for DesktopSystemManager {
  fn next_button(&self) -> Pin<Box<dyn std::future::Future<Output = SystemMessage> + Send + '_>> {
    Box::pin(async { SYSTEM_SIGNAL.wait().await })
  }
  fn inject(&self, msg: SystemMessage) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(async move {
      SYSTEM_SIGNAL.signal(msg);
    })
  }
}

/// Device events from the simulated keyboard hexpansion, shared between the
/// minifb thread (producer) and the menu task (consumer). Mirrors the
/// firmware's `EventQueue<DeviceEvent, 32>` in `HardwareHexpansionManager`.
static DEVICE_EVENT_CHANNEL: Channel<CriticalSectionRawMutex, DeviceEvent, 32> = Channel::new();

#[derive(Debug)]
pub struct DesktopHexpansionManager;

impl DesktopHexpansionManager {
  /// Called from the minifb thread when a keyboard hexpansion key is pressed/released.
  pub fn push_device_event(event: DeviceEvent) {
    // Drop on overflow, same as the firmware driver's `try_push`.
    let _ = DEVICE_EVENT_CHANNEL.try_send(event);
  }
}

impl HexpansionManager for DesktopHexpansionManager {
  fn next_event(&self) -> Pin<Box<dyn std::future::Future<Output = HexpansionEvent> + Send + '_>> {
    Box::pin(async { std::future::pending::<HexpansionEvent>().await })
  }

  fn try_next_event(&self) -> Option<HexpansionEvent> {
    None
  }

  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)> {
    Vec::new()
  }

  fn next_device_event(&self) -> Pin<Box<dyn std::future::Future<Output = DeviceEvent> + Send + '_>> {
    Box::pin(async { DEVICE_EVENT_CHANNEL.receive().await })
  }

  fn try_next_device_event(&self) -> Option<DeviceEvent> {
    DEVICE_EVENT_CHANNEL.try_receive().ok()
  }
}
