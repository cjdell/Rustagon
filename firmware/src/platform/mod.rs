pub mod hardware;
pub mod led;
pub mod mock;
pub mod power;
pub mod traits;

pub use hardware::HardwarePlatform;
pub use led::{
  BreatheEffect, ChaseEffect, FireEffect, HardwareLedManager, LedEffect, LedHandle, LedManager,
  OffEffect, RainbowEffect, SolidEffect, SparkleEffect, TheaterChaseEffect,
};
pub use mock::MockPlatform;
pub use power::{HardwarePowerManager, MockPowerManager, PowerError, PowerHandle, PowerManager};
pub use traits::Platform;
