pub mod storage;

use crate::config::storage::ConfigFileStorage;
use alloc::{boxed::Box, format, string::String, string::ToString, sync::Arc, vec::Vec};
use core::{fmt, pin::Pin};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use log::{info, warn};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Clone)]
pub struct ConfigFile<State, STORAGE: ConfigFileStorage> {
  storage: STORAGE,
  state: Arc<RwLock<CriticalSectionRawMutex, State>>,
}

impl<STATE: Clone + DeserializeOwned + Serialize, STORAGE: ConfigFileStorage> fmt::Debug for ConfigFile<STATE, STORAGE> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ConfigFile").finish()
  }
}

/// Object-safe config persistence trait.
pub trait ConfigFileTrait<STATE: Clone + DeserializeOwned + Serialize>: Send + Sync + fmt::Debug {
  fn get_json(&self) -> Pin<Box<dyn Future<Output = Result<String, StateError>> + Send + '_>>;
  fn set_json(&self, json: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>>;

  fn get_data(&self) -> Pin<Box<dyn Future<Output = STATE> + Send + '_>>;
  fn set_data(&self, new_state: STATE) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

  fn save(&self) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>>;
}

impl<STATE: Clone + DeserializeOwned + Serialize, STORAGE: ConfigFileStorage> ConfigFile<STATE, STORAGE> {
  pub async fn new(storage: STORAGE, initial: STATE) -> Self {
    let mut instance = Self {
      storage,
      state: Arc::new(RwLock::new(initial)),
    };

    instance.init().await;

    instance
  }

  async fn init(&mut self) {
    let json = match self.read_json().await {
      Ok(json) => json,
      Err(err) => {
        warn!("ConfigFile: Could not read JSON! {:?}", err);
        return;
      }
    };

    let state = match serde_json::from_str::<STATE>(&json) {
      Ok(state) => state,
      Err(err) => {
        warn!("ConfigFile: Could not decode JSON! {:?} {}", err, json.as_str());
        return;
      }
    };

    *self.state.write().await = state;
  }

  async fn read_json(&self) -> Result<String, StateError> {
    self
      .storage
      .read_json()
      .await
      .map_err(|_| StateError::Error("Read text file error".to_string()))
  }
}

#[derive(Debug)]
pub enum StateError {
  Error(String),
}

impl<STATE: Clone + DeserializeOwned + Serialize + Send + Sync, STORAGE: ConfigFileStorage + Send + Sync> ConfigFileTrait<STATE>
  for ConfigFile<STATE, STORAGE>
{
  fn get_json(&self) -> Pin<Box<dyn Future<Output = Result<String, StateError>> + Send + '_>> {
    Box::pin(async move {
      let state = self.state.read().await;
      serde_json::to_string::<STATE>(&state).map_err(|err| StateError::Error(format!("{err:?}")))
    })
  }

  fn set_json(&self, json: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>> {
    Box::pin(async move {
      let mut state = self.state.write().await;
      *state = serde_json::from_slice::<STATE>(&json).map_err(|err| StateError::Error(format!("{err:?}")))?;
      Ok(())
    })
  }

  fn get_data(&self) -> Pin<Box<dyn Future<Output = STATE> + Send + '_>> {
    Box::pin(async move { self.state.read().await.clone() })
  }

  fn set_data(&self, new_state: STATE) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
    Box::pin(async move {
      let mut state = self.state.write().await;
      *state = new_state;
    })
  }

  fn save(&self) -> Pin<Box<dyn Future<Output = Result<(), StateError>> + Send + '_>> {
    Box::pin(async move {
      let json = {
        let state = self.state.read().await;
        serde_json::to_string::<STATE>(&state).map_err(|err| StateError::Error(format!("{err:?}")))?
      };

      info!("ConfigFile.save: {}", json);

      self
        .storage
        .write_json(json)
        .await
        .map_err(|_| StateError::Error("Read text file error".to_string()))?;

      Ok(())
    })
  }
}
