use alloc::{string::String, vec::Vec, boxed::Box, sync::Arc};
use core::fmt;
use core::net::Ipv4Addr;
use core::pin::Pin;
use serde::{Deserialize, Serialize};

/// WiFi network scan result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WifiResult {
  pub ssid: String,
  pub signal_strength: i8,
  pub password_required: bool,
}

/// Current WiFi connection status
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiStatus {
  Offline,
  Connecting,
  Connected(Ipv4Addr),
  AccessPoint,
  Interrupted,
  NoNetworksFound,
}

/// Desired WiFi state
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum WifiDesiredState {
  Online,
  Offline,
}

/// WiFi mode (Station or Access Point)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiMode {
  Station,
  AccessPoint,
}

/// Connection statistics
#[derive(Clone, Debug, Default)]
pub struct WifiStats {
  pub connection_attempts: u32,
  pub successful_connections: u32,
}

/// WiFi Manager trait - manages WiFi connectivity and scanning
/// 
/// The manager runs continuously in the background, attempting to maintain
/// a WiFi connection if desired_state is Online, and reconnecting if interrupted.
/// 
/// Note: We use Pin<Box<dyn Future>> instead of async fn in trait because:
/// - async fn in traits requires #[async_trait] or nightly features
/// - We need dyn trait objects for the WiFiHandle wrapper
/// - async trait methods are not object-safe in stable Rust
pub trait WiFiManager: Send + Sync + fmt::Debug {
  /// Get the current WiFi status
  fn get_status(&self) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>>;

  /// Wait for the next status change and return the new status
  fn wait_for_status_change(
    &self,
  ) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>>;

  /// Get connection statistics
  fn get_stats(&self) -> WifiStats;

  /// Set the desired WiFi state (Online or Offline)
  /// The manager will work towards this state in the background
  fn set_desired_state(
    &self,
    state: WifiDesiredState,
  ) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>>;

  /// Scan for available WiFi networks
  fn scan(&self) -> Pin<Box<dyn core::future::Future<Output = Vec<WifiResult>> + Send + '_>>;

  /// Set WiFi mode (requires restart to take effect)
  fn set_wifi_mode(
    &self,
    mode: WifiMode,
  ) -> Pin<Box<dyn core::future::Future<Output = Result<(), &'static str>> + Send + '_>>;
}

/// Handle to WiFi Manager (Arc-wrapped for shared ownership)
pub struct WiFiHandle {
  inner: alloc::sync::Arc<dyn WiFiManager>,
}

impl WiFiHandle {
  pub fn new<T: WiFiManager + 'static>(manager: T) -> Self {
    Self {
      inner: alloc::sync::Arc::new(manager),
    }
  }

  pub fn get_status(
    &self,
  ) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>> {
    self.inner.get_status()
  }

  pub fn wait_for_status_change(
    &self,
  ) -> Pin<Box<dyn core::future::Future<Output = WifiStatus> + Send + '_>> {
    self.inner.wait_for_status_change()
  }

  pub fn get_stats(&self) -> WifiStats {
    self.inner.get_stats()
  }

  pub fn set_desired_state(
    &self,
    state: WifiDesiredState,
  ) -> Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    self.inner.set_desired_state(state)
  }

  pub fn scan(&self) -> Pin<Box<dyn core::future::Future<Output = Vec<WifiResult>> + Send + '_>> {
    self.inner.scan()
  }

  pub fn set_wifi_mode(
    &self,
    mode: WifiMode,
  ) -> Pin<Box<dyn core::future::Future<Output = Result<(), &'static str>> + Send + '_>> {
    self.inner.set_wifi_mode(mode)
  }
}

impl Clone for WiFiHandle {
  fn clone(&self) -> Self {
    Self {
      inner: self.inner.clone(),
    }
  }
}

impl fmt::Debug for WiFiHandle {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("WiFiHandle").finish()
  }
}
