use crate::lib::WifiResult;
use crate::tasks::menu::types::ItemType;
use crate::tasks::wifi::{ScanWatch, WifiCommandMessage, WifiCommandSender};
use crate::utils::local_fs::FileEntry;
use crate::{tasks::menu::types::MenuOption, utils::local_fs::LocalFs};
use alloc::vec;
use alloc::{boxed::Box, string::String, vec::Vec};
use esp_hal::peripherals::Peripherals;
use esp_storage::FlashStorage;

// Menu provider trait for both static and dynamic menus
pub trait MenuProvider {
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
  wifi_command_sender: WifiCommandSender,
  scan_signal: &'static ScanWatch,
  results: Option<Vec<WifiResult>>,
}

impl DynamicWifiMenu {
  pub fn new(wifi_command_sender: WifiCommandSender, scan_signal: &'static ScanWatch) -> Self {
    Self {
      wifi_command_sender,
      scan_signal,
      results: None,
    }
  }
}

impl MenuProvider for DynamicWifiMenu {
  async fn get_items(&mut self) -> Vec<MenuOption> {
    if self.results.is_none() {
      let mut scan_receiver = self.scan_signal.receiver().unwrap();

      self.wifi_command_sender.send(WifiCommandMessage::Scan).await;

      let results = match timeout_result!(scan_receiver.get(), 5_000, "Scan") {
        Ok(results) => results,
        Err(_) => vec![],
      };

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
