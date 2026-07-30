use crate::{
  apps::common::AppName,
  apps::{MenuAppAsync, MenuAppInput, MenuAppContext},
  platform::{HttpEventChannel, Platform},
  protocol::{HttpEvent, HttpRequest},
  types::*,
  utils::sleep,
};
use alloc::{
  format,
  string::{String, ToString},
  vec,
  vec::Vec,
};
use embassy_futures::join::join;
use serde::{Deserialize, Serialize};

pub struct AppStoreApp<P: Platform> {
  ctx: MenuAppContext<P>,
  state: AppState,
}

impl<P: Platform> AppName for AppStoreApp<P> {
  fn app_name() -> &'static str {
    "App Store"
  }
}

enum Screen {
  Welcome,
  Loading,
  AppList,
  AppInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppEntry {
  name: String,
  size: u32,
}

type AppList = Vec<AppEntry>;

struct AppState {
  screen: Screen,
  app_list: Option<AppList>,
  selected_app_index: usize,
  cursor: usize,
}

impl AppState {
  fn new() -> Self {
    Self { screen: Screen::Welcome, app_list: None, selected_app_index: 0, cursor: 0 }
  }

  fn reset_cursor(&mut self) { self.cursor = 0; }
  fn move_cursor_up(&mut self) { if self.cursor > 0 { self.cursor -= 1; } }
  fn move_cursor_down(&mut self, max: usize) { if self.cursor + 1 < max { self.cursor += 1; } }
  fn app_count(&self) -> usize { self.app_list.as_ref().map(|l| l.len()).unwrap_or(0) }
  fn current_app(&self) -> Option<&AppEntry> { self.app_list.as_ref()?.get(self.selected_app_index) }
}

impl<P: Platform> AppStoreApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self { ctx, state: AppState::new() }
  }

  async fn download_manifest(&mut self) -> Result<AppList, ()> {
    let req = HttpRequest::new(format!(
      "{}/manifest.json",
      self.ctx.platform.config_manager().get_data().await.app_store_url,
    ));

    let http_client = self.ctx.platform.http_client().ok_or(())?;
    let channel = HttpEventChannel::new();
    let mut meta = None;
    let mut body = Vec::new();

    join(
      http_client.request(req, &channel),
      async {
        loop {
          match channel.receive().await {
            HttpEvent::Meta(m) => meta = Some(m),
            HttpEvent::Chunk(chunk) => body.extend(chunk),
            HttpEvent::Done => break,
            HttpEvent::Error => return,
          }
        }
      },
    )
    .await;

    let _meta = meta.ok_or(())?;
    serde_json::from_slice::<AppList>(&body).map_err(|_| ())
  }

  fn render(&self) {
    let screen = match &self.state.screen {
      Screen::Welcome => LcdScreen::Headline(Icon40::Info, "Press B to refresh".to_string()),
      Screen::Loading => LcdScreen::Progress("Loading apps...".to_string()),
      Screen::AppList => LcdScreen::Menu {
        menu: self
          .state
          .app_list
          .as_ref()
          .map(|apps| apps.iter().map(|app| MenuLine(Icon20::File, app.name.clone())).collect())
          .unwrap_or_default(),
        selected: self.state.cursor as u32,
      },
      Screen::AppInfo => {
        if let Some(app) = self.state.current_app() {
          LcdScreen::Menu {
            menu: vec![
              MenuLine(Icon20::Info, format!("Name: {}", app.name)),
              MenuLine(Icon20::Info, format!("Size: {}", app.size)),
              MenuLine(Icon20::Info, "Download".to_string()),
              MenuLine(Icon20::Info, "Back".to_string()),
            ],
            selected: self.state.cursor as u32 + 2,
          }
        } else {
          LcdScreen::Headline(Icon40::Error, "App not found".to_string())
        }
      }
    };
    self.ctx.update_lcd(screen);
  }

  async fn handle_welcome_input(&mut self, input: HexButton) {
    if let HexButton::Right = input {
      self.state.screen = Screen::Loading;
      self.render();

      let app_list = match self.download_manifest().await {
        Ok(app_list) => app_list,
        Err(()) => {
          self.ctx.update_lcd(LcdScreen::Headline(Icon40::Error, "Manifest Error!".to_string()));
          sleep(1_000).await;
          return;
        }
      };

      self.state.screen = Screen::AppList;
      self.state.app_list = Some(app_list);
      self.state.reset_cursor();
    }
  }

  async fn handle_app_list_input(&mut self, input: HexButton) {
    match input {
      HexButton::Up => self.state.move_cursor_up(),
      HexButton::Down => self.state.move_cursor_down(self.state.app_count()),
      HexButton::Right => {
        self.state.screen = Screen::Loading;
        self.render();

        let app_list = match self.download_manifest().await {
          Ok(app_list) => app_list,
          Err(()) => {
            self.ctx.update_lcd(LcdScreen::Headline(Icon40::Error, "Manifest Error!".to_string()));
            sleep(1_000).await;
            return;
          }
        };

        self.state.screen = Screen::AppList;
        self.state.app_list = Some(app_list);
        self.state.reset_cursor();
      }
      HexButton::Fire => {
        self.state.selected_app_index = self.state.cursor;
        self.state.screen = Screen::AppInfo;
        self.state.cursor = 0;
      }
      _ => {}
    }
  }

  async fn handle_app_info_input(&mut self, input: HexButton) {
    match input {
      HexButton::Up => self.state.move_cursor_up(),
      HexButton::Down => self.state.move_cursor_down(2),
      HexButton::Fire => {
        let current_app = self.state.current_app().unwrap().clone();
        if self.state.cursor == 0 {
          // Download selected
          if let Err(()) = self.download(&current_app).await {
            self.ctx.update_lcd(LcdScreen::Headline(Icon40::Error, "Download Error!".to_string()));
            sleep(1_000).await;
          }
        } else {
          // Back
          self.state.screen = Screen::AppList;
          self.state.cursor = self.state.selected_app_index;
        }
      }
      _ => {}
    }
  }

  async fn download(&self, app: &AppEntry) -> Result<(), ()> {
    let http_client = self.ctx.platform.http_client().ok_or(())?;

    let req = HttpRequest::new(format!(
      "{}/{}",
      self.ctx.platform.config_manager().get_data().await.app_store_url,
      app.name
    ));

    let channel = HttpEventChannel::new();
    let mut bytes_written = 0u64;
    let app_name = app.name.clone();
    let display = self.ctx.platform.display_manager();
    let storage = self.ctx.platform.storage_manager();

    join(
      http_client.request(req, &channel),
      async {
        loop {
          match channel.receive().await {
            HttpEvent::Meta(_) => {}
            HttpEvent::Chunk(chunk) => {
              let len = chunk.len() as u64;
              let _ = display.signal(LcdScreen::BoundedProgress(bytes_written as u32, app.size));
              let _ = storage.write_binary_chunk(app_name.clone(), bytes_written as u32, chunk, false).await;
              bytes_written += len;
            }
            HttpEvent::Done => {
              let _ = display.signal(LcdScreen::BoundedProgress(bytes_written as u32, app.size));
              return;
            }
            HttpEvent::Error => return,
          }
        }
      },
    )
    .await;

    Ok(())
  }
}

impl<P: Platform> MenuAppAsync for AppStoreApp<P> {
  async fn work(&mut self) -> bool {
    loop {
      self.render();
      match self.ctx.input_receiver.receive().await {
        MenuAppInput::HexButton(input) => match &self.state.screen {
          Screen::Welcome => self.handle_welcome_input(input).await,
          Screen::Loading => {}
          Screen::AppList => self.handle_app_list_input(input).await,
          Screen::AppInfo => self.handle_app_info_input(input).await,
        },
        MenuAppInput::Stop => return false,
        _ => {}
      }
    }
  }
}
