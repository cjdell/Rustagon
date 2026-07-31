pub mod common;
pub mod config;
pub mod display;
pub mod fs;
pub mod input;

pub use common::*;
pub use config::DesktopConfigManager;
pub use display::DesktopDisplayManager;
pub use fs::DesktopLocalFs;
pub use input::DesktopInputManager;

use app::platform::hexpansion::HexpansionHandle;
use app::platform::storage::ConfigFileTrait;
use app::platform::*;
use app::types::{DeviceConfig, OtaError};
use core::{fmt, future::Future, pin::Pin};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct DesktopHttpClient;

impl app::platform::HttpClient for DesktopHttpClient {
    fn request<'a>(
        &'a self,
        req: app::protocol::HttpRequest,
        channel: &'a app::platform::HttpEventChannel,
    ) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
        let url = req.url;
        let method = req.method;
        let body = req.body;
        Box::pin(async move {
            use app::protocol::{HttpEvent, HttpMethod};

            let response = match method {
                HttpMethod::Get => match ureq::get(&url).call() {
                    Ok(r) => r,
                    Err(_) => { channel.send(HttpEvent::Error).await; return; }
                },
                HttpMethod::Post => match ureq::post(&url).send(body.as_slice()) {
                    Ok(r) => r,
                    Err(_) => { channel.send(HttpEvent::Error).await; return; }
                },
                HttpMethod::Put => match ureq::put(&url).send(body.as_slice()) {
                    Ok(r) => r,
                    Err(_) => { channel.send(HttpEvent::Error).await; return; }
                },
                HttpMethod::Delete => match ureq::delete(&url).call() {
                    Ok(r) => r,
                    Err(_) => { channel.send(HttpEvent::Error).await; return; }
                },
            };

            let status = response.status().as_u16() as u32;
            let meta = app::protocol::HttpResponseMeta::new(status);
            channel.send(HttpEvent::Meta(meta)).await;

            let body = response.into_body();
            let mut reader = body.into_reader();
            let mut buf = vec![0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        channel.send(HttpEvent::Chunk(buf[..n].to_vec())).await;
                    }
                    Err(_) => {
                        channel.send(HttpEvent::Error).await;
                        return;
                    }
                }
            }

            channel.send(HttpEvent::Done).await;
        })
    }
}

#[derive(Clone)]
pub struct DesktopPlatform {
    pub display_raw: Arc<DesktopDisplayManager>,
    display: DisplayHandle,
    hexpansion: HexpansionHandle,
    led: LedHandle,
    power: PowerHandle,
    wifi: WiFiHandle,
    input: InputHandle,
    system: SystemHandle,
    storage: StorageHandle,
    config: ConfigHandle<DeviceConfig>,
    http_client: HttpClientHandle,
}

impl fmt::Debug for DesktopPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DesktopPlatform").finish()
    }
}

impl DesktopPlatform {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let display_raw = Arc::new(DesktopDisplayManager::new());
        let display = DisplayHandle::new(display_raw.clone() as Arc<dyn DisplayManager>);
        let hexpansion = HexpansionHandle::new(Arc::new(DesktopHexpansionManager) as Arc<dyn HexpansionManager>);
        let led = LedHandle::new(Arc::new(DesktopLedManager) as Arc<dyn LedManager>);
        let power = PowerHandle::new(Arc::new(DesktopPowerManager) as Arc<dyn PowerManager>);
        let wifi = WiFiHandle::new(Arc::new(DesktopWifiManager) as Arc<dyn WiFiManager>);
        let input = InputHandle::new(Arc::new(DesktopInputManager::new()) as Arc<dyn InputManager>);
        let system = SystemHandle::new(Arc::new(DesktopSystemManager) as Arc<dyn SystemManager>);
        let local_fs = DesktopLocalFs::new(data_dir);
        let storage = StorageHandle::new(Arc::new(local_fs) as Arc<dyn LocalFsTrait>);
        let config = ConfigHandle::new(Arc::new(DesktopConfigManager::new()) as Arc<dyn ConfigFileTrait<DeviceConfig>>);
        let http_client = HttpClientHandle::new(Arc::new(DesktopHttpClient) as Arc<dyn app::platform::HttpClient>);

        Self { display_raw, display, hexpansion, led, power, wifi, input, system, storage, config, http_client }
    }

    pub fn get_screen(&self) -> (display_types::LcdScreen, i64) {
        self.display_raw.get_screen()
    }
}

impl Platform for DesktopPlatform {
    fn display_manager(&self) -> DisplayHandle { self.display.clone() }
    fn hexpansion_manager(&self) -> HexpansionHandle { self.hexpansion.clone() }
    fn led_manager(&self) -> LedHandle { self.led.clone() }
    fn power_manager(&self) -> PowerHandle { self.power.clone() }
    fn wifi_manager(&self) -> WiFiHandle { self.wifi.clone() }
    fn input_manager(&self) -> InputHandle { self.input.clone() }
    fn system_manager(&self) -> SystemHandle { self.system.clone() }
    fn http_client(&self) -> Option<HttpClientHandle> { Some(self.http_client.clone()) }
    fn storage_manager(&self) -> StorageHandle { self.storage.clone() }
    fn config_manager(&self) -> ConfigHandle<DeviceConfig> { self.config.clone() }
    async fn format_storage(&self) -> Result<(), FsError> { self.storage.format().await }
    async fn software_reset(&self) { std::process::exit(0); }
    async fn ota_begin(&self) -> Result<u32, OtaError> { Err(OtaError::NotSupported) }
    async fn ota_write_chunk(&self, _: u32, _: &[u8]) -> Result<(), OtaError> { Err(OtaError::NotSupported) }
    async fn ota_commit(&self) -> Result<(), OtaError> { Err(OtaError::NotSupported) }
}
