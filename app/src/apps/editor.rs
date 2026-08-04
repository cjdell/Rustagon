use crate::{
  apps::{AppAction, AppEvent, MenuApp, MenuAppContext, MenuAppInput, common::AppName},
  platform::Platform,
  types::*,
};
use alloc::{string::String, vec, vec::Vec};
use log::info;

/// Number of lines in the editor's fixed buffer window. The `TextBuffer`
/// `LcdScreen` mode displays up to 8 lines, so the window is 8 lines deep.
const BUFFER_LINES: usize = 8;

pub struct EditorApp<P: Platform> {
  ctx: MenuAppContext<P>,
  /// The editor's fixed 8-line window. Each entry is one line of text; lines
  /// grow as you type. When the buffer overflows (e.g. Enter on the last line)
  /// the oldest line is dropped — the window rotates, matching how a future
  /// file-backed editor will page through a larger file.
  lines: Vec<String>,
  /// Index of the currently selected line within `lines`.
  active: usize,
  /// Per-line cursor positions (byte index into the corresponding `lines` entry).
  cursors: Vec<usize>,
}

impl<P: Platform> AppName for EditorApp<P> {
  fn app_name() -> &'static str {
    "Editor"
  }
}

impl<P: Platform> EditorApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self {
      ctx,
      lines: vec![String::new(); BUFFER_LINES],
      active: 0,
      cursors: vec![0; BUFFER_LINES],
    }
  }

  fn handle_key(&mut self, code: KeyCode, typ: KeyEventType) {
    if typ == KeyEventType::Released {
      return;
    }
    info!("EditorApp: handling key {code:?}");
    match code {
      KeyCode::Backspace => {
        if self.cursors[self.active] > 0 {
          let line = &mut self.lines[self.active];
          line.remove(self.cursors[self.active] - 1);
          self.cursors[self.active] -= 1;
        }
      }
      KeyCode::Delete => {
        let cursor = self.cursors[self.active];
        let line = &mut self.lines[self.active];
        if cursor < line.len() {
          line.remove(cursor);
        }
      }
      KeyCode::Left => {
        if self.cursors[self.active] > 0 {
          self.cursors[self.active] -= 1;
        }
      }
      KeyCode::Right => {
        if self.cursors[self.active] < self.lines[self.active].len() {
          self.cursors[self.active] += 1;
        }
      }
      KeyCode::Up => {
        if self.active > 0 {
          self.active -= 1;
          self.clamp_cursor();
        }
      }
      KeyCode::Down => {
        if self.active < self.lines.len() - 1 {
          self.active += 1;
          self.clamp_cursor();
        }
      }
      KeyCode::Home => {
        self.cursors[self.active] = 0;
      }
      KeyCode::End => {
        self.cursors[self.active] = self.lines[self.active].len();
      }
      KeyCode::Enter => {
        // Split the current line at the cursor; the tail becomes a new line
        // below it. Keep the window at 8 lines by rotating the oldest line out.
        let tail = self.lines[self.active].split_off(self.cursors[self.active]);
        self.lines.insert(self.active + 1, tail);
        self.cursors.insert(self.active + 1, 0);
        if self.lines.len() > BUFFER_LINES {
          // Dropping the top line shifts everything (including the inserted
          // tail) up one slot, so the tail lands back at `self.active`.
          self.lines.remove(0);
          self.cursors.remove(0);
        } else {
          self.active += 1;
        }
        self.cursors[self.active] = 0;
      }
      KeyCode::Space => {
        self.insert_char(' ');
      }
      KeyCode::Tab => {
        self.insert_str("  ");
      }
      _ => {
        if let Some(ch) = keycode_to_char(code) {
          self.insert_char(ch);
        }
      }
    }
  }

  fn insert_char(&mut self, ch: char) {
    self.lines[self.active].insert(self.cursors[self.active], ch);
    self.cursors[self.active] += 1;
  }

  fn insert_str(&mut self, s: &str) {
    self.lines[self.active].insert_str(self.cursors[self.active], s);
    self.cursors[self.active] += s.len();
  }

  fn clamp_cursor(&mut self) {
    let len = self.lines[self.active].len();
    if self.cursors[self.active] > len {
      self.cursors[self.active] = len;
    }
  }
}

fn keycode_to_char(code: KeyCode) -> Option<char> {
  match code {
    KeyCode::A => Some('a'),
    KeyCode::B => Some('b'),
    KeyCode::C => Some('c'),
    KeyCode::D => Some('d'),
    KeyCode::E => Some('e'),
    KeyCode::F => Some('f'),
    KeyCode::G => Some('g'),
    KeyCode::H => Some('h'),
    KeyCode::I => Some('i'),
    KeyCode::J => Some('j'),
    KeyCode::K => Some('k'),
    KeyCode::L => Some('l'),
    KeyCode::M => Some('m'),
    KeyCode::N => Some('n'),
    KeyCode::O => Some('o'),
    KeyCode::P => Some('p'),
    KeyCode::Q => Some('q'),
    KeyCode::R => Some('r'),
    KeyCode::S => Some('s'),
    KeyCode::T => Some('t'),
    KeyCode::U => Some('u'),
    KeyCode::V => Some('v'),
    KeyCode::W => Some('w'),
    KeyCode::X => Some('x'),
    KeyCode::Y => Some('y'),
    KeyCode::Z => Some('z'),
    KeyCode::Digit0 => Some('0'),
    KeyCode::Digit1 => Some('1'),
    KeyCode::Digit2 => Some('2'),
    KeyCode::Digit3 => Some('3'),
    KeyCode::Digit4 => Some('4'),
    KeyCode::Digit5 => Some('5'),
    KeyCode::Digit6 => Some('6'),
    KeyCode::Digit7 => Some('7'),
    KeyCode::Digit8 => Some('8'),
    KeyCode::Digit9 => Some('9'),
    KeyCode::Comma => Some(','),
    KeyCode::Period => Some('.'),
    KeyCode::Slash => Some('/'),
    KeyCode::Semicolon => Some(';'),
    KeyCode::Quote => Some('\''),
    KeyCode::Minus => Some('-'),
    KeyCode::Equals => Some('='),
    KeyCode::Backtick => Some('`'),
    KeyCode::Backslash => Some('\\'),
    KeyCode::LBracket => Some('['),
    KeyCode::RBracket => Some(']'),
    _ => None,
  }
}

impl<P: Platform> MenuApp for EditorApp<P> {
  fn render(&self) -> LcdScreen {
    let lines = self
      .lines
      .iter()
      .enumerate()
      .map(|(i, text)| TextBufferLine {
        text: text.clone(),
        cursor: (i == self.active).then_some(self.cursors[i] as u32),
      })
      .collect();
    LcdScreen::TextBuffer { lines }
  }

  async fn init(&mut self) {
    self.ctx.update_lcd(self.render());
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    // Editing is driven by the keyboard hexpansion; hex buttons do not edit.
    // Exit via the boot button (the menu runner pops the app on a system button).
    match input {
      MenuAppInput::Stop => AppAction::Stop,
      _ => AppAction::Continue,
    }
  }

  async fn handle_event(&mut self, event: AppEvent) {
    // Process the event that triggered this call first (it was already consumed
    // from the queue by the menu loop), then drain any additional events.
    if let AppEvent::Device(DeviceEvent::Keyboard(ke)) = event {
      self.handle_key(ke.code, ke.typ);
    }
  }
}
