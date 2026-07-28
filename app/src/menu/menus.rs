use crate::menu::types::MenuOption;
use alloc::vec::Vec;

pub trait MenuProvider {
  async fn get_items(&mut self) -> Vec<MenuOption>;
}

pub struct StaticMenu {
  pub items: Vec<MenuOption>,
}

impl MenuProvider for StaticMenu {
  async fn get_items(&mut self) -> Vec<MenuOption> {
    self.items.clone()
  }
}
