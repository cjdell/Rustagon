use super::traits::*;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;
use core::pin::Pin;
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use crate::utils::WatchedValue;

/// Mock WiFi Manager for testing (no hardware required)
#[derive(Clone)]
pub struct MockWifiManager {
  status: WatchedValue<WifiStatus>,
  desired_state: Arc<RwLock<CriticalSectionRawMutex, WifiDesiredState>>,
  connection_attempts: Arc<AtomicU32>,
  successful_connections: Arc<AtomicU32>,
}

impl MockWifiManager {
  pub fn new() -> Self {
    Self {
      status: WatchedValue::new(WifiStatus::Offline),
      desired_state: Arc::new(RwLock::new(WifiDesiredState::Offline)),
      connection_attempts: Arc::new(AtomicU32::new(0)),
      successful_connections: Arc::new(AtomicU32::new(0)),
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
    Box::pin(async move {
      status.get().await
    })
  }

  fn wait_for_status_change(
    &self,
  ) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>> {
    let status = self.status.clone();
    Box::pin(async move {
      status.wait_for_change().await
    })
  }



  fn get_stats(&self) -> WifiStats {
    WifiStats {
      connection_attempts: self.connection_attempts.load(Ordering::Relaxed),
      successful_connections: self.successful_connections.load(Ordering::Relaxed),
    }
  }

  fn set_desired_state(
    &self,
    state: WifiDesiredState,
  ) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    let desired_state = self.desired_state.clone();
    Box::pin(async move {
      *desired_state.write().await = state;
    })
  }

  fn scan(&self) -> Pin<Box<dyn core::future::Future<Output = Vec<WifiResult>> + Send + '_>> {
    Box::pin(async {
      // Mock: return empty list
      Vec::new()
    })
  }

  fn set_wifi_mode(
    &self,
    _mode: WifiMode,
  ) -> Pin<Box<dyn core::future::Future<Output = Result<(), &'static str>> + Send + '_>> {
    Box::pin(async { Ok(()) })
  }
}
