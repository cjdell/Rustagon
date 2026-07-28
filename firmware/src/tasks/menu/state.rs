use crate::{
  apps::{MenuAppInput, MenuAppType},
  native::NativeAppType,
  platform::Platform,
  tasks::menu::{menus::*, types::*},
  types::*,
};
use alloc::{borrow::ToOwned as _, boxed::Box, format, string::ToString as _, sync::Arc, vec, vec::Vec};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, rwlock::RwLock};

pub struct MenuState {
  pub ctx: MenuContext,
  pub app: Arc<RwLock<NoopRawMutex, AppState>>,

  pub current_menu: Menu,
  pub menu_options: Vec<MenuOption>,
  pub selected: u32,

  pub http_message: HttpStatusMessage,
}

pub enum AppState {
  None,
  MenuApp,
  HostedApp,
}

impl MenuState {
  pub async fn refresh(&mut self) {
    let app = &self.app.clone();
    if let Ok(app) = app.try_read() {
      match *app {
        AppState::None => {
          self.draw_menu().await;
        }
        AppState::MenuApp => {
          self.ctx.menu_app_input_channel.send(MenuAppInput::Refresh).await;
        }
        AppState::HostedApp => {
          // Hosted apps draw themselves
        }
      };
    }
  }

  pub async fn get_menu_provider(&mut self) -> MenuTypes {
    match &self.current_menu {
      Menu::Root => MenuTypes::StaticMenu(Box::new(StaticMenu {
        items: vec![
          MenuAppType::list_apps()
            .iter()
            .map(|name| MenuOption::App {
              name,
              app_type: AppType::MenuApp,
            })
            .collect(),
          NativeAppType::list_apps()
            .iter()
            .map(|name| MenuOption::App {
              name,
              app_type: AppType::NativeApp,
            })
            .collect(),
          vec![
            MenuOption::PowerOff,
            MenuOption::Menu {
              menu: Menu::Files("/".to_string()),
            },
          ],
        ]
        .concat(),
      })),
      Menu::Files(path) => MenuTypes::DynamicFilesystemMenu(Box::new(DynamicFilesystemMenu {
        storage: self.ctx.storage.clone(),
        path: path.clone(),
      })),
    }
  }

  pub fn get_menu_screen(&self, menu: &Vec<MenuOption>) -> LcdScreen {
    match self.http_message {
      HttpStatusMessage::Progress(transferred, total) => {
        return LcdScreen::BoundedProgress(transferred, total);
      }
      _ => (),
    }

    LcdScreen::Menu {
      menu: menu
        .iter()
        .map(|option| match option {
          MenuOption::App { name, app_type: _ } => MenuLine(Icon20::Info, name.to_string()),
          MenuOption::Menu { menu } => MenuLine(Icon20::Info, menu.label().to_string()),
          MenuOption::Item { name, item_type } => match item_type {
            ItemType::File => MenuLine(Icon20::File, format!("{}", name)),
            ItemType::Directory => MenuLine(Icon20::File, format!("{}", name)),
          },
          MenuOption::Back => MenuLine(Icon20::Info, "<= Back".to_owned()),
          MenuOption::PowerOff => MenuLine(Icon20::Info, "Power Off".to_owned()),
        })
        .collect(),
      selected: self.selected,
    }
  }

  pub async fn draw_menu(&mut self) {
    // Ensure selected is within bounds
    if self.selected >= self.menu_options.len() as u32 {
      self.selected = if self.menu_options.is_empty() {
        0
      } else {
        self.menu_options.len() as u32 - 1
      };
    }

    let _ = self.ctx.display.signal(self.get_menu_screen(&self.menu_options));
  }
}
