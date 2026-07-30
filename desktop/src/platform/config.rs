use app::platform::storage::ConfigFileTrait;
use app::platform::StateError;
use app::types::DeviceConfig;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use std::fs;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

const CONFIG_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/data");

#[derive(Debug)]
pub struct DesktopConfigManager {
    state: Arc<RwLock<CriticalSectionRawMutex, DeviceConfig>>,
    file_path: PathBuf,
}

impl DesktopConfigManager {
    pub fn new() -> Self {
        let file_path = PathBuf::from(CONFIG_DIR).join("device.jsn");
        fs::create_dir_all(file_path.parent().unwrap()).ok();

        let initial = fs::read_to_string(&file_path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();

        Self {
            state: Arc::new(RwLock::new(initial)),
            file_path,
        }
    }
}

impl ConfigFileTrait<DeviceConfig> for DesktopConfigManager {
    fn get_json(&self) -> Pin<Box<dyn Future<Output = Result<String, StateError>> + Send + '_>> {
        Box::pin(async move {
            let state = self.state.read().await;
            serde_json::to_string(&*state).map_err(|e| StateError::Error(format!("{e:?}")))
        })
    }

    fn set_json(&self, json: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>> {
        Box::pin(async move {
            let mut state = self.state.write().await;
            *state =
                serde_json::from_slice(&json).map_err(|e| StateError::Error(format!("{e:?}")))?;
            Ok(())
        })
    }

    fn get_data(&self) -> Pin<Box<dyn Future<Output = DeviceConfig> + Send + '_>> {
        Box::pin(async move { self.state.read().await.clone() })
    }

    fn set_data(&self, new_state: DeviceConfig) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            let mut state = self.state.write().await;
            *state = new_state;
        })
    }

    fn save(&self) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>> {
        let file_path = self.file_path.clone();
        Box::pin(async move {
            let json = {
                let state = self.state.read().await;
                serde_json::to_string(&*state).map_err(|e| StateError::Error(format!("{e:?}")))?
            };
            fs::write(&file_path, &json).map_err(|e| StateError::Error(format!("{e:?}")))?;
            Ok(())
        })
    }
}
