use crate::{
  apps::{
    MenuAppAsync, MenuAppInput,
    common::{AppName, MenuAppContext},
  },
  platform::Platform,
  protocol::HostIpcMessage,
  types::*,
  utils::sleep,
};
use alloc::{format, string::String, string::ToString, vec, vec::Vec};
use embedded_tools::local_fs::DirEntry;
use log::{info, warn};

pub struct FilesApp {
  ctx: MenuAppContext,
  state: AppState,
}

impl AppName for FilesApp {
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
  message: Option<String>,
}

impl AppState {
  fn new() -> Self {
    Self {
      screen: Screen::FileList,
      files: Vec::new(),
      cursor: 0,
      selected_file: None,
      message: None,
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

  /// Number of selectable items on the FileDetail screen
  fn detail_action_count(file: &DirEntry) -> usize {
    let mut count = 0usize;
    if Self::is_wasm(&file.name) {
      count += 1; // Execute
    }
    count += 1; // Delete
    count += 1; // Back
    count
  }
}

impl FilesApp {
  pub fn new(ctx: MenuAppContext) -> Self {
    Self {
      ctx,
      state: AppState::new(),
    }
  }

  async fn refresh_files(&mut self) {
    self.state.files = self.ctx.platform.storage_manager().list_files().await.unwrap_or_default();
  }

  async fn set_message(&mut self, msg: &str) {
    self.state.message = Some(msg.to_string());
    self.ctx.update_lcd(self.render());
    sleep(2_000).await;
    self.state.message = None;
  }

  fn render(&self) -> LcdScreen {
    if let Some(ref msg) = self.state.message {
      return LcdScreen::Headline(Icon40::Info, msg.clone());
    }

    match &self.state.screen {
      Screen::FileList => {
        let mut menu: Vec<MenuLine> = self.state.files.iter().map(|f| MenuLine(Icon20::File, format!("{}  {}B", f.name, f.size))).collect();
        menu.push(MenuLine(Icon20::Info, "<= Back".to_string()));
        LcdScreen::Menu {
          menu,
          selected: self.state.cursor as u32,
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
          }
        } else {
          LcdScreen::Headline(Icon40::Error, "File not found".to_string())
        }
      }
    }
  }

  /// Returns true if the app should stop (Back selected)
  async fn handle_file_list_input(&mut self, input: HexButton) -> bool {
    let max = self.state.files.len() + 1; // files + Back
    match input {
      HexButton::Up => self.state.move_cursor_up(),
      HexButton::Down => self.state.move_cursor_down(max),
      HexButton::Fire => {
        if self.state.cursor < self.state.files.len() {
          let file = self.state.files[self.state.cursor].clone();
          info!("Selected file: {}", file.name);
          self.state.selected_file = Some(file);
          self.state.cursor = 0;
          self.state.screen = Screen::FileDetail;
        } else {
          return true; // Back
        }
      }
      _ => {}
    }
    false
  }

  /// Returns true if the app should stop (Back selected or WASM launched)
  async fn handle_file_detail_input(&mut self, input: HexButton) -> bool {
    let file = match &self.state.selected_file {
      Some(f) => f.clone(),
      None => return false,
    };
    let info_lines = 2;
    let max = AppState::detail_action_count(&file);

    match input {
      HexButton::Up => self.state.move_cursor_up(),
      HexButton::Down => self.state.move_cursor_down(max + info_lines),
      HexButton::Fire => {
        if self.state.cursor < info_lines {
          return false; // Info line, no action
        }
        // Map cursor to action index
        let action_idx = self.state.cursor - info_lines;
        let has_execute = AppState::is_wasm(&file.name);

        if has_execute && action_idx == 0 {
          info!("Executing WASM: {}", file.name);
          self.ctx.host_ipc_sender.send((0, HostIpcMessage::StartWasm(file.name))).await;
          return true; // App handled by WASM runtime now
        }

        let delete_idx = if has_execute { 1 } else { 0 };
        if action_idx == delete_idx {
          info!("Deleting file: {}", file.name);
          match self.ctx.platform.storage_manager().delete(file.name).await {
            Ok(()) => {
              warn!("Deleted");
              self.state.screen = Screen::FileList;
              self.state.cursor = 0;
              self.refresh_files().await;
            }
            Err(err) => {
              warn!("Delete error: {err:?}");
              self.set_message("Delete failed!").await;
            }
          }
          return false;
        }

        // Back (last action)
        self.state.screen = Screen::FileList;
        self.state.cursor = 0;
      }
      HexButton::Right | HexButton::Left => {
        self.state.screen = Screen::FileList;
        self.state.cursor = 0;
      }
      _ => {}
    }
    false
  }
}

impl MenuAppAsync for FilesApp {
  async fn work(&mut self) -> bool {
    self.refresh_files().await;

    loop {
      self.ctx.update_lcd(self.render());

      match self.ctx.input_receiver.receive().await {
        MenuAppInput::HexButton(input) => {
          let should_stop = match &self.state.screen {
            Screen::FileList => self.handle_file_list_input(input).await,
            Screen::FileDetail => self.handle_file_detail_input(input).await,
          };
          if should_stop {
            return false;
          }
        }
        MenuAppInput::Refresh => {
          self.refresh_files().await;
        }
        MenuAppInput::Stop => return false,
        _ => {}
      }
    }
  }
}
