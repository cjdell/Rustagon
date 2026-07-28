use crate::{
  apps::*,
  platform::Platform,
  protocol::*,
  tasks::menu::{menus::MenuProvider as _, state::MenuState, types::*},
  types::*,
};
use alloc::string::ToString as _;
use log::info;

impl MenuState {
  pub async fn execute_option(&mut self) -> () {
    let mut new_menu: Option<Menu> = None;

    match &self.menu_options[self.selected as usize] {
      MenuOption::App { name, app_type } => {
        info!("Open {name}");

        match app_type {
          AppType::MenuApp => {
            self.ctx.menu_app_input_channel.send(MenuAppInput::Start(name.to_string())).await;
          }
          AppType::NativeApp => {
            self.ctx.host_ipc_sender.send((0, HostIpcMessage::StartNative(name.to_string()))).await;
          }
        }
      }
      MenuOption::Back => new_menu = Some(Menu::Root),
      MenuOption::PowerOff => {
        self.ctx.platform.power_manager().power_off().await;
      }
    };

    if let Some(new_menu) = new_menu {
      self.current_menu = new_menu;
      self.menu_options = self.get_menu_provider().await.get_items().await;
      self.selected = 0; // Reset selection when changing menus
    }
  }
}
