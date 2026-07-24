pub mod traits;
pub use traits::{WiFiHandle, WiFiManager, WifiDesiredState, WifiMode, WifiResult, WifiStats, WifiStatus};

pub mod hardware;
pub mod mock;

pub use hardware::HardwareWifiManager;
pub use mock::MockWifiManager;
