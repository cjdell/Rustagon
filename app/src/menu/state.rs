use crate::{
  apps::{MenuAppInput, MenuAppType},
  menu::{menus::*, types::*},
  native::NativeAppType,
  platform::Platform,
  types::*,
};
use alloc::{borrow::ToOwned as _, string::ToString as _, sync::Arc, vec, vec::Vec};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, rwlock::RwLock};

pub struct MenuState<P: Platform> {
  pub ctx: MenuContext<P>,
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

impl<P: Platform> MenuState<P> {
  pub async fn refresh(&mut self) {
    let app = &self.app.clone();
    if let Ok(app) = app.try_read() {
      match *app {
        AppState::None => self.draw_menu().await,
        AppState::MenuApp => {
          self.ctx.menu_app_input_channel.send(MenuAppInput::Refresh).await;
        }
        AppState::HostedApp => {}
      };
    }
  }

  pub async fn get_menu_provider(&mut self) -> StaticMenu {
    StaticMenu {
      items: vec![
        MenuAppType::<P>::list_apps().iter()
          .map(|name| MenuOption::App { name, app_type: AppType::MenuApp })
          .collect(),
        NativeAppType::list_apps().iter()
          .map(|name| MenuOption::App { name, app_type: AppType::NativeApp })
          .collect(),
        vec![MenuOption::PowerOff],
      ].concat(),
    }
  }

  pub fn get_menu_screen(&self, menu: &[MenuOption]) -> LcdScreen {
    match self.http_message {
      HttpStatusMessage::Progress(transferred, total) => return LcdScreen::BoundedProgress(transferred, total),
      _ => (),
    }
    LcdScreen::Menu {
      menu: menu.iter().map(|option| match option {
        MenuOption::App { name, .. } => MenuLine(Icon20::Info, name.to_string()),
        MenuOption::Back => MenuLine(Icon20::Info, "<= Back".to_owned()),
        MenuOption::PowerOff => MenuLine(Icon20::Info, "Power Off".to_owned()),
      }).collect(),
      selected: self.selected,
    }
  }

  pub async fn draw_menu(&mut self) {
    if self.selected >= self.menu_options.len() as u32 {
      self.selected = if self.menu_options.is_empty() { 0 } else { self.menu_options.len() as u32 - 1 };
    }
    let _ = self.ctx.display.signal(self.get_menu_screen(&self.menu_options));
  }
}
