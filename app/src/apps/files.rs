use crate::{
  apps::{AppAction, MenuApp, MenuAppContext, MenuAppInput, common::AppName},
  platform::Platform,
  types::*,
};
use alloc::{format, string::ToString, vec, vec::Vec};
use embedded_tools::local_fs::DirEntry;
use log::{info, warn};

pub struct FilesApp<P: Platform> {
  ctx: MenuAppContext<P>,
  state: AppState,
}

impl<P: Platform> AppName for FilesApp<P> {
  fn app_name() -> &'static str {
    "Files"
  }
}

enum Screen {
  FileList,
  FileDetail,
}

struct AppState {
  screen: Screen,
  files: Vec<DirEntry>,
  cursor: usize,
  selected_file: Option<DirEntry>,
  /// True after navigating back from FileDetail to FileList, so the list
  /// slides in from the left (back direction) instead of from the right.
  back_nav: bool,
}

impl AppState {
  fn new() -> Self {
    Self {
      screen: Screen::FileList,
      files: Vec::new(),
      cursor: 0,
      selected_file: None,
      back_nav: false,
    }
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
  fn is_wasm(name: &str) -> bool {
    name.ends_with(".wsm") || name.ends_with(".wasm")
  }
  fn detail_action_count(file: &DirEntry) -> usize {
    let mut count = 0usize;
    if Self::is_wasm(&file.name) {
      count += 1;
    }
    count += 1;
    count += 1;
    count
  }
}

impl<P: Platform> FilesApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self {
      ctx,
      state: AppState::new(),
    }
  }

  async fn refresh_files(&mut self) {
    self.state.files = self.ctx.platform.storage_manager().list_files().await.unwrap_or_default();
  }
}

impl<P: Platform> MenuApp for FilesApp<P> {
  fn render(&self) -> LcdScreen {
    match &self.state.screen {
      Screen::FileList => {
        let mut menu: Vec<MenuLine> = self
          .state
          .files
          .iter()
          .map(|f| MenuLine(Icon20::File, format!("{}  {}B", f.name, f.size)))
          .collect();
        menu.push(MenuLine(Icon20::Info, "<= Back".to_string()));
        let animation = if self.state.back_nav {
          MenuAnimation::FromLeft
        } else {
          MenuAnimation::FromRight
        };
        LcdScreen::Menu {
          menu,
          selected: self.state.cursor as u32,
          animation,
        }
      }
      Screen::FileDetail => {
        if let Some(file) = &self.state.selected_file {
          let mut items = vec![
            MenuLine(Icon20::Info, format!("Name: {}", file.name)),
            MenuLine(Icon20::Info, format!("Size: {}B", file.size)),
          ];
          if AppState::is_wasm(&file.name) {
            items.push(MenuLine(Icon20::Config, "Execute".to_string()));
          }
          items.push(MenuLine(Icon20::Config, "Delete".to_string()));
          items.push(MenuLine(Icon20::Info, "<= Back".to_string()));
          LcdScreen::Menu {
            menu: items,
            selected: self.state.cursor as u32,
            animation: MenuAnimation::FromRight,
          }
        } else {
          LcdScreen::Headline(Icon40::Error, "File not found".to_string())
        }
      }
    }
  }

  async fn init(&mut self) {
    self.refresh_files().await;
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    match input {
      MenuAppInput::Stop => AppAction::Stop,
      MenuAppInput::Button(hex) => match &self.state.screen {
        Screen::FileList => self.handle_file_list_input(hex).await,
        Screen::FileDetail => self.handle_file_detail_input(hex).await,
      },
    }
  }
}

impl<P: Platform> FilesApp<P> {
  async fn handle_file_list_input(&mut self, input: HexButton) -> AppAction {
    let max = self.state.files.len() + 1;
    match input {
      HexButton::Up => self.state.move_cursor_up(),
      HexButton::Down => self.state.move_cursor_down(max),
      HexButton::Fire => {
        if self.state.cursor < self.state.files.len() {
          self.state.selected_file = Some(self.state.files[self.state.cursor].clone());
          self.state.cursor = 0;
          self.state.screen = Screen::FileDetail;
        } else {
          return AppAction::Stop;
        }
      }
      _ => {}
    }
    AppAction::Continue
  }

  async fn handle_file_detail_input(&mut self, input: HexButton) -> AppAction {
    let file = match &self.state.selected_file {
      Some(f) => f.clone(),
      None => return AppAction::Stop,
    };
    let info_lines = 2;
    let max = AppState::detail_action_count(&file);
    match input {
      HexButton::Up => self.state.move_cursor_up(),
      HexButton::Down => self.state.move_cursor_down(max + info_lines),
      HexButton::Fire => {
        if self.state.cursor < info_lines {
          return AppAction::Continue;
        }
        let action_idx = self.state.cursor - info_lines;
        let has_execute = AppState::is_wasm(&file.name);
        if has_execute && action_idx == 0 {
          info!("Executing WASM: {}", file.name);
          return AppAction::LaunchWasm(file.name);
        }
        let delete_idx = if has_execute { 1 } else { 0 };
        if action_idx == delete_idx {
          match self.ctx.platform.storage_manager().delete(file.name).await {
            Ok(()) => {
              self.state.screen = Screen::FileList;
              self.state.cursor = 0;
              self.state.back_nav = true;
              self.refresh_files().await;
            }
            Err(err) => {
              warn!("Delete error: {err:?}");
              self.ctx.notify("Delete failed!", Icon40::Error).await;
            }
          }
          return AppAction::Continue;
        }
        self.state.screen = Screen::FileList;
        self.state.cursor = 0;
        self.state.back_nav = true;
      }
      HexButton::Right | HexButton::Left => {
        self.state.screen = Screen::FileList;
        self.state.cursor = 0;
        self.state.back_nav = true;
      }
      _ => {}
    }
    AppAction::Continue
  }
}
