use crate::{
  apps::{common::AppName, AppAction, AppEvent, MenuApp, MenuAppInput, MenuAppContext},
  platform::Platform,
  types::*,
};
use alloc::{format, string::String, string::ToString, vec::Vec};
use log::info;

pub struct EditorApp<P: Platform> {
  ctx: MenuAppContext<P>,
  buffer: String,
  cursor: usize,
}

impl<P: Platform> AppName for EditorApp<P> {
  fn app_name() -> &'static str {
    "Editor"
  }
}

impl<P: Platform> EditorApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self { ctx, buffer: String::new(), cursor: 0 }
  }

  fn handle_key(&mut self, code: KeyCode, typ: KeyEventType) {
    if typ == KeyEventType::Released {
      return;
    }
    info!("EditorApp: handling key {code:?}");
    match code {
      KeyCode::Backspace => {
        if self.cursor > 0 {
          self.buffer.remove(self.cursor - 1);
          self.cursor -= 1;
        }
      }
      KeyCode::Delete => {
        if self.cursor < self.buffer.len() {
          self.buffer.remove(self.cursor);
        }
      }
      KeyCode::Left => { if self.cursor > 0 { self.cursor -= 1; } }
      KeyCode::Right => { if self.cursor < self.buffer.len() { self.cursor += 1; } }
      KeyCode::Home => { self.cursor = 0; }
      KeyCode::End => { self.cursor = self.buffer.len(); }
      KeyCode::Enter => { self.buffer.insert(self.cursor, '\n'); self.cursor += 1; }
      KeyCode::Space => { self.buffer.insert(self.cursor, ' '); self.cursor += 1; }
      KeyCode::Tab => { self.buffer.insert_str(self.cursor, "  "); self.cursor += 2; }
      _ => {
        if let Some(ch) = keycode_to_char(code) {
          self.buffer.insert(self.cursor, ch);
          self.cursor += 1;
        }
      }
    }
  }

  fn drain_device_events(&mut self) {
    while let Some(event) = self.ctx.platform.hexpansion_manager().try_next_device_event() {
      let DeviceEvent::Keyboard(ke) = event;
      info!("EditorApp: device event {ke:?}");
      self.handle_key(ke.code, ke.typ);
    }
  }
}

fn keycode_to_char(code: KeyCode) -> Option<char> {
  match code {
    KeyCode::A => Some('a'), KeyCode::B => Some('b'),
    KeyCode::C => Some('c'), KeyCode::D => Some('d'),
    KeyCode::E => Some('e'), KeyCode::F => Some('f'),
    KeyCode::G => Some('g'), KeyCode::H => Some('h'),
    KeyCode::I => Some('i'), KeyCode::J => Some('j'),
    KeyCode::K => Some('k'), KeyCode::L => Some('l'),
    KeyCode::M => Some('m'), KeyCode::N => Some('n'),
    KeyCode::O => Some('o'), KeyCode::P => Some('p'),
    KeyCode::Q => Some('q'), KeyCode::R => Some('r'),
    KeyCode::S => Some('s'), KeyCode::T => Some('t'),
    KeyCode::U => Some('u'), KeyCode::V => Some('v'),
    KeyCode::W => Some('w'), KeyCode::X => Some('x'),
    KeyCode::Y => Some('y'), KeyCode::Z => Some('z'),
    KeyCode::Digit0 => Some('0'), KeyCode::Digit1 => Some('1'),
    KeyCode::Digit2 => Some('2'), KeyCode::Digit3 => Some('3'),
    KeyCode::Digit4 => Some('4'), KeyCode::Digit5 => Some('5'),
    KeyCode::Digit6 => Some('6'), KeyCode::Digit7 => Some('7'),
    KeyCode::Digit8 => Some('8'), KeyCode::Digit9 => Some('9'),
    KeyCode::Comma => Some(','), KeyCode::Period => Some('.'),
    KeyCode::Slash => Some('/'), KeyCode::Semicolon => Some(';'),
    KeyCode::Quote => Some('\''), KeyCode::Minus => Some('-'),
    KeyCode::Equals => Some('='), KeyCode::Backtick => Some('`'),
    KeyCode::Backslash => Some('\\'),
    KeyCode::LBracket => Some('['), KeyCode::RBracket => Some(']'),
    _ => None,
  }
}

impl<P: Platform> MenuApp for EditorApp<P> {
  fn render(&self) -> LcdScreen {
    let mut lines: Vec<MenuLine> = Vec::new();
    lines.push(MenuLine(Icon20::Info, "Editor".to_string()));
    lines.push(MenuLine(Icon20::Info, "".to_string()));

    let display_text = if self.buffer.len() > 80 {
      format!("{}...", &self.buffer[..80])
    } else if self.buffer.is_empty() {
      "Type to begin...".to_string()
    } else {
      self.buffer.clone()
    };
    lines.push(MenuLine(Icon20::Info, display_text));

    lines.push(MenuLine(Icon20::Info, "".to_string()));
    lines.push(MenuLine(Icon20::Info, "<= Back".to_string()));

    LcdScreen::Menu { menu: lines, selected: 0 }
  }

  async fn init(&mut self) {
    self.drain_device_events();
    self.ctx.update_lcd(self.render());
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    self.drain_device_events();
    match input {
      MenuAppInput::Stop => AppAction::Stop,
      MenuAppInput::Button(HexButton::Fire | HexButton::Left) => AppAction::Stop,
      _ => AppAction::Continue,
    }
  }

  async fn handle_event(&mut self, _event: AppEvent) {
    self.drain_device_events();
  }
}
