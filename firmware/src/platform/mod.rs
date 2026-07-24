use crate::{platform::power::PowerManager, utils::MaskedI2cBus};

pub mod power;

pub struct Platform {
  power_manager: PowerManager<MaskedI2cBus>,
}

impl Platform {
  pub fn new(sys_bus: MaskedI2cBus) -> Self {
    let power_manager = PowerManager::new(sys_bus);

    Self { power_manager }
  }
}
