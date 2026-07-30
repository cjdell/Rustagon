use crate::apps::MenuAppType;
use crate::menu::types::{AppType, MenuOption};
use crate::platform::Platform;
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

pub fn get_root_menu_options<P: Platform>(additional_apps: &[&'static str]) -> Vec<MenuOption> {
  let mut items: Vec<MenuOption> = MenuAppType::<P>::list_apps().iter()
    .map(|name| MenuOption::App { name, app_type: AppType::MenuApp })
    .collect();

  items.extend(
    additional_apps.iter()
      .map(|name| MenuOption::App { name, app_type: AppType::MenuApp })
  );

  items.push(MenuOption::PowerOff);
  items
}
