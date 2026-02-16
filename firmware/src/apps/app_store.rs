use crate::{
  apps::{
    MenuAppAsync, MenuAppInput,
    common::{AppName, MenuAppContext},
  },
  lib::{HexButton, Icon20, Icon40, LcdScreen, MenuLine},
  utils::{
    http::{HttpEvent, HttpRequest, perform_http_request, perform_http_request_channel},
    sleep,
  },
};
use alloc::vec;
use alloc::{
  format,
  string::{String, ToString},
  vec::Vec,
};
use core::future::join;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Channel};
use log::error;
use serde::{Deserialize, Serialize};

pub struct AppStoreApp {
  ctx: MenuAppContext,
  state: AppState,
}

impl AppName for AppStoreApp {
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

#[derive(Debug, Clone)]
enum AppInfoOption {
  Download,
  Back,
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
  selected_app_index: usize, // Which app is selected in the list
  cursor: usize,             // Current cursor position in current screen
}

impl AppState {
  fn new() -> Self {
    Self {
      screen: Screen::Welcome,
      app_list: None,
      selected_app_index: 0,
      cursor: 0,
    }
  }

  fn reset_cursor(&mut self) {
    self.cursor = 0;
  }

  fn move_cursor_up(&mut self) {
    if self.cursor > 0 {
      self.cursor -= 1;
    }
  }

  fn move_cursor_down(&mut self, max: usize) {
    if self.cursor + 1 < max {
      self.cursor += 1;
    }
  }

  fn app_count(&self) -> usize {
    self.app_list.as_ref().map(|l| l.len()).unwrap_or(0)
  }

  fn current_app(&self) -> Option<&AppEntry> {
    self.app_list.as_ref()?.get(self.selected_app_index)
  }

  fn app_info_options() -> &'static [AppInfoOption] {
    use AppInfoOption::*;
    &[Download, Back]
  }

  fn default_option_index(_option: AppInfoOption) -> usize {
    Self::app_info_options().iter().position(|o| matches!(o, _option)).unwrap_or(0)
  }
}

impl AppStoreApp {
  pub fn new(ctx: MenuAppContext) -> Self {
    Self {
      ctx,
      state: AppState::new(),
    }
  }

  async fn download_manifest(&mut self) -> Result<AppList, anyhow::Error> {
    let req = HttpRequest::new(format!(
      "{}/manifest.json",
      self.ctx.device_state.get_data().app_store_url,
    ));

    let res = perform_http_request(self.ctx.stack, req).await.map_err(|_| anyhow::anyhow!("HTTP err"))?;

    let app_list = serde_json::from_slice::<AppList>(&res.body)?;

    Ok(app_list)
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
              MenuLine(Icon20::Info, "Download".to_string()), // These should be derived from the enum rather than magic strings
              MenuLine(Icon20::Info, "Back".to_string()),
            ],
            selected: self.state.cursor as u32 + 2, // First 2 items aren't selectable
          }
        } else {
          LcdScreen::Headline(Icon40::Error, "App not found".to_string())
        }
      }
    };

    self.ctx.update_lcd(screen);
  }

  async fn handle_welcome_input(&mut self, input: HexButton) {
    if let HexButton::B = input {
      self.state.screen = Screen::Loading;
      self.render();

      let app_list = match self.download_manifest().await {
        Ok(app_list) => app_list,
        Err(err) => {
          error!("Manifest Error: {err:?}");
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
      HexButton::A => self.state.move_cursor_up(),
      HexButton::D => self.state.move_cursor_down(self.state.app_count()),
      HexButton::B => {
        self.state.screen = Screen::Loading;
        self.render();

        let app_list = match self.download_manifest().await {
          Ok(app_list) => app_list,
          Err(err) => {
            error!("Manifest Error: {err:?}");
            self.ctx.update_lcd(LcdScreen::Headline(Icon40::Error, "Manifest Error!".to_string()));
            sleep(1_000).await;
            return;
          }
        };

        self.state.screen = Screen::AppList;
        self.state.app_list = Some(app_list);
        self.state.reset_cursor();
      }
      HexButton::C => {
        // Save which app was selected
        self.state.selected_app_index = self.state.cursor;
        // Enter app info screen, set cursor to Download option
        self.state.screen = Screen::AppInfo;
        self.state.cursor = AppState::default_option_index(AppInfoOption::Download);
      }
      _ => {}
    }
  }

  async fn handle_app_info_input(&mut self, input: HexButton) {
    match input {
      HexButton::A => self.state.move_cursor_up(),
      HexButton::D => self.state.move_cursor_down(AppState::app_info_options().len()),
      HexButton::C => {
        use AppInfoOption::*;
        let current_app = self.state.current_app().unwrap();
        let option = &AppState::app_info_options()[self.state.cursor];
        match option {
          Download => {
            if let Err(err) = self.download(&current_app).await {
              error!("Download Error: {err:?}");
              self.ctx.update_lcd(LcdScreen::Headline(Icon40::Error, "Download Error!".to_string()));
              sleep(1_000).await;
            }
          }
          Back => {
            // Return to app list, restore cursor to the app we were viewing
            self.state.screen = Screen::AppList;
            self.state.cursor = self.state.selected_app_index;
          }
        }
      }
      _ => {}
    }
  }

  async fn download(&self, app: &AppEntry) -> Result<(), anyhow::Error> {
    let channel = Channel::<NoopRawMutex, HttpEvent, 1>::new();

    let req = HttpRequest::new(format!(
      "{}/{}",
      self.ctx.device_state.get_data().app_store_url,
      app.name
    ));

    let request = perform_http_request_channel(self.ctx.stack, channel.sender(), &req);
    let mut bytes_written = 0u64;

    let listen = async {
      loop {
        match channel.receive().await {
          HttpEvent::Meta(_) => (),
          HttpEvent::Chunk(chunk) => {
            self.ctx.update_lcd(LcdScreen::BoundedProgress(bytes_written as u32, app.size));
            self.ctx.local_fs.write_binary_chunk(&app.name, bytes_written, &chunk, false).unwrap();
            bytes_written += chunk.len() as u64;
          }
          HttpEvent::Done => {
            self.ctx.update_lcd(LcdScreen::BoundedProgress(bytes_written as u32, app.size));
            return;
          }
        }
      }
    };

    let (result, _) = join!(request, listen).await;
    if let Err(_err) = result {
      return Err(anyhow::Error::msg("Download error"));
    };
    Ok(())
  }
}

impl MenuAppAsync for AppStoreApp {
  async fn work(&mut self) -> bool {
    loop {
      self.render();

      match self.ctx.input_receiver.receive().await {
        MenuAppInput::HexButton(input) => {
          match &self.state.screen {
            Screen::Welcome => self.handle_welcome_input(input).await,
            Screen::Loading => {} // Ignore input while loading
            Screen::AppList => self.handle_app_list_input(input).await,
            Screen::AppInfo => self.handle_app_info_input(input).await,
          }
        }
        MenuAppInput::Stop => {
          return false;
        }
        _ => {}
      }
    }
  }
}
