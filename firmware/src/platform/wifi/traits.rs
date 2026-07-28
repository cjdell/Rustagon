pub use app::platform::wifi::{WiFiHandle, WiFiManager, WifiStatus};
pub use app::types::{WifiDesiredState, WifiMode, WifiResult};

/// Connection statistics
#[derive(Clone, Debug, Default)]
pub struct WifiStats {
  pub connection_attempts: u32,
  pub successful_connections: u32,
}
