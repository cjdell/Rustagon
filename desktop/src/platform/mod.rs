pub mod display;
pub mod input;
mod mock;

pub use display::DesktopDisplayManager;
pub use input::DesktopInputManager;

use crate::MockConfigManager;
use app::platform::storage::ConfigFileTrait;
use app::platform::*;
use app::types::{DeviceConfig, OtaError};
use core::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct DesktopPlatform {
    pub display_raw: Arc<DesktopDisplayManager>,
    display: DisplayHandle,
    led: LedHandle,
    power: PowerHandle,
    wifi: WiFiHandle,
    input: InputHandle,
    system: SystemHandle,
    storage: StorageHandle,
    config: ConfigHandle<DeviceConfig>,
}

impl fmt::Debug for DesktopPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DesktopPlatform").finish()
    }
}

impl DesktopPlatform {
    pub fn new() -> Self {
        let display_raw = Arc::new(DesktopDisplayManager::new());
        let display = DisplayHandle::new(display_raw.clone() as Arc<dyn DisplayManager>);
        let led = LedHandle::new(Arc::new(mock::MockLedManager) as Arc<dyn LedManager>);
        let power = PowerHandle::new(Arc::new(mock::MockPowerManager) as Arc<dyn PowerManager>);
        let wifi = WiFiHandle::new(Arc::new(mock::MockWifiManager) as Arc<dyn WiFiManager>);
        let input = InputHandle::new(Arc::new(DesktopInputManager::new()) as Arc<dyn InputManager>);
        let system = SystemHandle::new(Arc::new(mock::MockSystemManager) as Arc<dyn SystemManager>);
        let storage = StorageHandle::new(Arc::new(mock::MockStorageManager) as Arc<dyn LocalFsTrait>);
        let config = ConfigHandle::new(Arc::new(MockConfigManager {}) as Arc<dyn ConfigFileTrait<DeviceConfig>>);

        Self { display_raw, display, led, power, wifi, input, system, storage, config }
    }

    pub fn get_screen(&self) -> (display_types::LcdScreen, i64) {
        self.display_raw.get_screen()
    }
}

impl Platform for DesktopPlatform {
    fn display_manager(&self) -> DisplayHandle { self.display.clone() }
    fn led_manager(&self) -> LedHandle { self.led.clone() }
    fn power_manager(&self) -> PowerHandle { self.power.clone() }
    fn wifi_manager(&self) -> WiFiHandle { self.wifi.clone() }
    fn input_manager(&self) -> InputHandle { self.input.clone() }
    fn system_manager(&self) -> SystemHandle { self.system.clone() }
    fn http_client(&self) -> Option<app::platform::HttpClientHandle> { None }
    fn storage_manager(&self) -> StorageHandle { self.storage.clone() }
    fn config_manager(&self) -> ConfigHandle<DeviceConfig> { self.config.clone() }
    async fn format_storage(&self) -> Result<(), FsError> { Ok(()) }
    async fn software_reset(&self) { std::process::exit(0); }
    async fn ota_begin(&self) -> Result<u32, OtaError> { Err(OtaError::NotSupported) }
    async fn ota_write_chunk(&self, _: u32, _: &[u8]) -> Result<(), OtaError> { Err(OtaError::NotSupported) }
    async fn ota_commit(&self) -> Result<(), OtaError> { Err(OtaError::NotSupported) }
}
