use crate::{tasks::menu::types::MenuOption};
use alloc::vec::Vec;

// Menu provider trait for both static and dynamic menus
pub(crate) trait MenuProvider {
  async fn get_items(&mut self) -> Vec<MenuOption>;
}

// Static menu provider
pub struct StaticMenu {
  pub items: Vec<MenuOption>,
}

impl MenuProvider for StaticMenu {
  async fn get_items(&mut self) -> Vec<MenuOption> {
    self.items.clone()
  }
}
