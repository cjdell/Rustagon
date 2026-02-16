use crate::utils::local_fs::LocalFs;
use alloc::{format, string::String, sync::Arc};
use core::cell::RefCell;
use log::{error, info};
use serde::{Serialize, de::DeserializeOwned};

pub struct PersistentStateService<State> {
  local_fs: LocalFs,
  file_name: String,
  state: Arc<RefCell<State>>,
}

impl<State> Clone for PersistentStateService<State> {
  fn clone(&self) -> Self {
    Self {
      local_fs: self.local_fs.clone(),
      file_name: self.file_name.clone(),
      state: Arc::clone(&self.state),
    }
  }
}

impl<State: Clone + DeserializeOwned + Serialize> PersistentStateService<State> {
  pub fn new(local_fs: LocalFs, file_name: String, initial: State) -> PersistentStateService<State> {
    PersistentStateService {
      local_fs,
      file_name,
      state: Arc::new(RefCell::new(initial)),
    }
  }

  fn read_json(&self) -> Result<String, StateError> {
    self
      .local_fs
      .read_text_file(&self.file_name)
      .map_err(|err| StateError::Error(format!("Read text file error {err:?}")))
  }

  pub fn init(&mut self) -> Result<(), StateError> {
    match self.read_json() {
      Ok(json) => {
        info!("PersistentStateService.init: {}", json);

        *self.state.borrow_mut() =
          serde_json::from_str::<State>(&json).map_err(|err| StateError::Error(format!("{err:?}")))?;

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
    *self.state.borrow_mut() =
      serde_json::from_slice::<State>(json).map_err(|err| StateError::Error(format!("{err:?}")))?;

    Ok(())
  }

  pub fn get_data(&self) -> State {
    self.state.borrow().clone()
  }

  pub fn set_data(&self, new_state: State) -> () {
    *self.state.borrow_mut() = new_state;
  }

  pub fn save(&self) -> Result<(), StateError> {
    let json = self.get_json()?;

    info!("PersistentStateService.save: {}", json);

    self.local_fs.write_text_file(&self.file_name, &json).map_err(|err| StateError::Error(format!("{err:?}")))?;

    Ok(())
  }
}

#[derive(Debug)]
pub enum StateError {
  Error(String),
}
