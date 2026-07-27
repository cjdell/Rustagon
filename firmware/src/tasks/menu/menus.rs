use crate::{
  platform::StorageHandle,
  tasks::menu::types::{ItemType, MenuOption},
  types::*,
};
use alloc::vec;
use alloc::{boxed::Box, string::String, vec::Vec};

// Menu provider trait for both static and dynamic menus
pub(crate) trait MenuProvider {
  async fn get_items(&mut self) -> Vec<MenuOption>;
}

pub enum MenuTypes {
  StaticMenu(Box<StaticMenu>),
  DynamicFilesystemMenu(Box<DynamicFilesystemMenu>),
}

impl MenuProvider for MenuTypes {
  async fn get_items(&mut self) -> Vec<MenuOption> {
    match self {
      MenuTypes::StaticMenu(menu) => menu.get_items().await,
      MenuTypes::DynamicFilesystemMenu(menu) => menu.get_items().await,
    }
  }
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

// Dynamic menu provider (example: filesystem)
pub struct DynamicFilesystemMenu {
  pub storage: StorageHandle,
  pub path: String,
}

impl MenuProvider for DynamicFilesystemMenu {
  async fn get_items(&mut self) -> Vec<MenuOption> {
    let files = self.storage.list_files().await.unwrap_or_default();

    vec![files.iter().map(|file| file.into()).collect(), vec![MenuOption::Back]].concat()
  }
}

impl Into<MenuOption> for &embedded_tools::local_fs::DirEntry {
  fn into(self) -> MenuOption {
    MenuOption::Item {
      name: self.name.clone(),
      item_type: ItemType::File,
    }
  }
}
