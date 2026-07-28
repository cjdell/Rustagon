use super::traits::*;
use crate::types::WifiDesiredState;
use crate::utils::WatchedValue;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::pin::Pin;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};

#[derive(Clone)]
pub struct MockWifiManager {
  status: WatchedValue<WifiStatus>,
  desired_state: Arc<RwLock<CriticalSectionRawMutex, WifiDesiredState>>,
}

impl MockWifiManager {
  pub fn new() -> Self {
    Self {
      status: WatchedValue::new(WifiStatus::Offline),
      desired_state: Arc::new(RwLock::new(WifiDesiredState::Offline)),
    }
  }
}

impl fmt::Debug for MockWifiManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockWifiManager").finish()
  }
}

impl WiFiManager for MockWifiManager {
  fn get_status(&self) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>> {
    let status = self.status.clone();
    Box::pin(async move { status.get().await })
  }

  fn wait_for_status_change(&self) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>> {
    let status = self.status.clone();
    Box::pin(async move { status.wait_for_change().await })
  }

  fn set_desired_state(&self, state: WifiDesiredState) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    let desired_state = self.desired_state.clone();
    Box::pin(async move { *desired_state.write().await = state; })
  }

  fn scan(&self) -> Pin<Box<dyn core::future::Future<Output = Result<Vec<WifiResult>, ()>> + Send + '_>> {
    Box::pin(async { Ok(Vec::new()) })
  }
}
