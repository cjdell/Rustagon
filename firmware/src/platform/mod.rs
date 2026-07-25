pub mod hardware;
pub mod input;
pub mod led;
pub mod mock;
pub mod power;
pub mod system;
pub mod traits;
pub mod wifi;

pub use hardware::HardwarePlatform;
pub use input::{HardwareInputManager, InputHandle, InputManager, MockInputManager};
pub use led::{
  BreatheEffect, ChaseEffect, FireEffect, HardwareLedManager, LedEffect, LedHandle, LedManager, OffEffect, RainbowEffect, SolidEffect,
  SparkleEffect, TheaterChaseEffect,
};
pub use mock::MockPlatform;
pub use power::{HardwarePowerManager, MockPowerManager, PowerError, PowerHandle, PowerManager};
pub use system::{HardwareSystemManager, SystemHandle};
pub use traits::Platform;
pub use wifi::{
  HardwareWifiManager, MockWifiManager, WiFiHandle, WiFiManager, WifiDesiredState, WifiMode, WifiResult, WifiStats, WifiStatus,
};
