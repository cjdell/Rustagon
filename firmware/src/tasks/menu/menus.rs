use crate::{
  platform::Platform,
  tasks::menu::types::{ItemType, MenuOption},
  timeout_result,
  types::*,
  utils::local_fs::{FileEntry, LocalFs},
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
  DynamicWifiMenu(Box<DynamicWifiMenu>),
}

impl MenuProvider for MenuTypes {
  async fn get_items(&mut self) -> Vec<MenuOption> {
    match self {
      MenuTypes::StaticMenu(menu) => menu.get_items().await,
      MenuTypes::DynamicFilesystemMenu(menu) => menu.get_items().await,
      MenuTypes::DynamicWifiMenu(menu) => menu.get_items().await,
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
  pub local_fs: LocalFs,
  pub path: String,
}

impl MenuProvider for DynamicFilesystemMenu {
  async fn get_items(&mut self) -> Vec<MenuOption> {
    let files = self.local_fs.dir().unwrap_or_default();

    vec![files.iter().map(|file| file.into()).collect(), vec![MenuOption::Back]].concat()
  }
}

impl Into<MenuOption> for &FileEntry {
  fn into(self) -> MenuOption {
    MenuOption::Item {
      name: self.name.clone(), //  format!("{} - {} bytes", self.name, self.size),
      item_type: ItemType::File,
    }
  }
}

// Dynamic menu provider (example: WiFi networks)
pub struct DynamicWifiMenu {
  platform: crate::platform::HardwarePlatform,
  results: Option<Vec<WifiResult>>,
}

impl DynamicWifiMenu {
  pub fn new(platform: crate::platform::HardwarePlatform) -> Self {
    Self {
      platform,
      results: None,
    }
  }
}

impl MenuProvider for DynamicWifiMenu {
  async fn get_items(&mut self) -> Vec<MenuOption> {
    if self.results.is_none() {
      // Scan WiFi networks via platform
      let platform_results = self.platform.wifi_manager().scan().await;
      
      // Convert from platform WifiResult to types::WifiResult
      let results: Vec<WifiResult> = platform_results
        .iter()
        .map(|r| WifiResult {
          ssid: r.ssid.clone(),
          signal_strength: r.signal_strength,
          password_required: r.password_required,
        })
        .collect();

      self.results = Some(results);
    }

    vec![
      self
        .results
        .as_ref()
        .unwrap()
        .iter()
        .map(|result| MenuOption::Item {
          name: result.ssid.clone(),
          item_type: ItemType::WifiNetwork {
            rssi: result.signal_strength as i32,
          },
        })
        .collect(),
      vec![MenuOption::Back],
    ]
    .concat()
  }
}
