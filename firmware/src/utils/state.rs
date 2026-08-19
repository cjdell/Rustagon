use crate::platform::StorageHandle;
use alloc::{format, string::String, sync::Arc};
use core::cell::RefCell;
use log::{error, info};
use serde::{Serialize, de::DeserializeOwned};

pub struct PersistentStateService<State> {
  storage: StorageHandle,
  file_name: String,
  state: Arc<RefCell<State>>,
}

impl<State> Clone for PersistentStateService<State> {
  fn clone(&self) -> Self {
    Self {
      storage: self.storage.clone(),
      file_name: self.file_name.clone(),
      state: Arc::clone(&self.state),
    }
  }
}

impl<State: Clone + DeserializeOwned + Serialize> PersistentStateService<State> {
  pub fn new(storage: StorageHandle, file_name: String, initial: State) -> PersistentStateService<State> {
    PersistentStateService {
      storage,
      file_name,
      state: Arc::new(RefCell::new(initial)),
    }
  }

  async fn read_json(&self) -> Result<String, StateError> {
    self
      .storage
      .read_text_file(self.file_name.clone())
      .await
      .map_err(|err| StateError::Error(format!("Read text file error {err:?}")))
  }

  pub async fn init(&mut self) -> Result<(), StateError> {
    match self.read_json().await {
      Ok(json) => {
        info!("PersistentStateService.init: {}", json);

        *self.state.borrow_mut() = serde_json::from_str::<State>(&json).map_err(|err| StateError::Error(format!("{err:?}")))?;

        Ok(())
      }
      Err(err) => {
        error!("PersistentStateService.init: Error: {err:?}");

        Ok(())
      }
    }
  }

  pub fn get_json(&self) -> Result<String, StateError> {
    serde_json::to_string::<State>(&self.state.borrow()).map_err(|err| StateError::Error(format!("{err:?}")))
  }

  pub fn set_json(&self, json: &[u8]) -> Result<(), StateError> {
    *self.state.borrow_mut() = serde_json::from_slice::<State>(json).map_err(|err| StateError::Error(format!("{err:?}")))?;

    Ok(())
  }

  pub fn get_data(&self) -> State {
    self.state.borrow().clone()
  }

  pub fn set_data(&self, new_state: State) {
    *self.state.borrow_mut() = new_state;
  }

  pub async fn save(&self) -> Result<(), StateError> {
    let json = self.get_json()?;

    info!("PersistentStateService.save: {}", json);

    self
      .storage
      .write_text_file(self.file_name.clone(), json)
      .await
      .map_err(|err| StateError::Error(format!("{err:?}")))?;

    Ok(())
  }
}

#[derive(Debug)]
pub enum StateError {
  Error(String),
}
