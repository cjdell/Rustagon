use app::platform::led::{LedError, LedManager};
use app::platform::power::{PowerManager, PowerStatus};
use app::platform::system::SystemManager;
use app::platform::wifi::{WiFiManager, WifiStatus};
use app::platform::storage::ConfigFileTrait;
use app::platform::{DirEntry, FileType, FsError, LocalFsTrait, StateError};
use app::types::{HexButton, LedRequest, SystemMessage, WifiDesiredState, WifiResult};
use core::fmt;
use std::pin::Pin;

#[derive(Debug)]
pub struct MockLedManager;
impl LedManager for MockLedManager {
    fn request(&self, _: LedRequest) -> Result<(), LedError> { Ok(()) }
}

#[derive(Debug)]
pub struct MockPowerManager;
impl PowerManager for MockPowerManager {
    fn power_off(&self) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async { std::process::exit(0); })
    }
    fn get_status(&self) -> Pin<Box<dyn std::future::Future<Output = PowerStatus> + Send + '_>> {
        Box::pin(async { PowerStatus {
            vbat_mv: 3700,
            vsys_mv: 3700,
            vbus_mv: 0,
            charge_current_ma: 0,
            charge_voltage_mv: 4200,
            input_current_limit_ma: 500,
            is_charging: false,
            is_power_present: false,
            battery_fault: false,
        }})
    }
}

#[derive(Debug)]
pub struct MockWifiManager;
impl WiFiManager for MockWifiManager {
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

#[derive(Debug)]
pub struct MockSystemManager;
impl SystemManager for MockSystemManager {
    fn next_button(&self) -> Pin<Box<dyn std::future::Future<Output = SystemMessage> + Send + '_>> {
        Box::pin(async { std::future::pending::<SystemMessage>().await })
    }
    fn inject(&self, _: SystemMessage) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}

#[derive(Debug)]
pub struct MockStorageManager;
impl LocalFsTrait for MockStorageManager {
    fn format(&self) -> Pin<Box<dyn std::future::Future<Output = Result<(), FsError>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn list_files(&self) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> { Box::pin(async { Ok(Vec::new()) }) }
    fn list_dir(&self, _: String) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<DirEntry>, FsError>> + Send + '_>> { Box::pin(async { Ok(Vec::new()) }) }
    fn get_file_size(&self, _: String) -> Pin<Box<dyn std::future::Future<Output = Result<u32, FsError>> + Send + '_>> { Box::pin(async { Err(FsError::NotFound) }) }
    fn read_binary_chunk(&self, _: String, _: u32, _: u32) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, FsError>> + Send + '_>> { Box::pin(async { Err(FsError::NotFound) }) }
    fn write_binary_chunk(&self, _: String, _: u32, _: Vec<u8>, _: bool) -> Pin<Box<dyn std::future::Future<Output = Result<(), FsError>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn read_text_file(&self, _: String) -> Pin<Box<dyn std::future::Future<Output = Result<String, FsError>> + Send + '_>> { Box::pin(async { Err(FsError::NotFound) }) }
    fn write_text_file(&self, _: String, _: String) -> Pin<Box<dyn std::future::Future<Output = Result<(), FsError>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn delete(&self, _: String) -> Pin<Box<dyn std::future::Future<Output = Result<(), FsError>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn mkdir(&self, _: String) -> Pin<Box<dyn std::future::Future<Output = Result<(), FsError>> + Send + '_>> { Box::pin(async { Ok(()) }) }
    fn file_exists(&self, _: String) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + '_>> { Box::pin(async { false }) }
    fn get_file_type(&self, _: String) -> Pin<Box<dyn std::future::Future<Output = Result<FileType, FsError>> + Send + '_>> { Box::pin(async { Err(FsError::NotFound) }) }
}
