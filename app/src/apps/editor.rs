use crate::{
  apps::{AppAction, AppEvent, AppInput, AppRunContext, AppRunEvent, MenuApp, MenuAppContext, common::AppName},
  platform::Platform,
  types::*,
  ui::text_input::TextInput,
};
use alloc::{string::ToString, vec, vec::Vec};
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
  lines: Vec<TextInput>,
  /// Index of the currently selected line within `lines`.
  active: usize,
  /// Whether a shift key is currently held. Toggled by `KeyCode::Shift`
  /// press/release events; applied to typed characters via `KeyCode::to_char`.
  shifted: bool,
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
      lines: vec![TextInput::new(); BUFFER_LINES],
      active: 0,
      shifted: false,
    }
  }

  fn handle_key(&mut self, code: KeyCode, typ: KeyEventType) {
    // Shift is tracked from both press and release to apply to characters
    // typed while it is held. All other keys act on press only.
    if code == KeyCode::Shift {
      self.shifted = typ == KeyEventType::Pressed;
      return;
    }
    if typ == KeyEventType::Released {
      return;
    }
    info!("EditorApp: handling key {code:?}");
    let line = &mut self.lines[self.active];
    match code {
      KeyCode::Backspace => line.backspace(),
      KeyCode::Delete => line.delete(),
      KeyCode::Home => line.home(),
      KeyCode::End => line.end(),
      KeyCode::Space => line.insert_char(' '),
      KeyCode::Tab => line.insert_str("  "),
      _ => {
        if let Some(ch) = code.to_char(self.shifted) {
          line.insert_char(ch);
        }
      }
    }
  }

  /// Handle a navigation button. Arrows and Fire/Enter arrive here as `HexButton`
  /// presses (identical to the physical badge buttons); character keys still
  /// arrive as keyboard events via [`handle_key`](Self::handle_key).
  fn handle_nav_button(&mut self, button: HexButton) {
    match button {
      HexButton::Left => self.lines[self.active].left(),
      HexButton::Right => self.lines[self.active].right(),
      HexButton::Up if self.active > 0 => self.active -= 1,
      HexButton::Down if self.active < self.lines.len() - 1 => self.active += 1,
      HexButton::Fire => {
        // Split the current line at the cursor; the tail becomes a new line
        // below it. Keep the window at 8 lines by rotating the oldest line out.
        let cursor = self.lines[self.active].cursor();
        let tail = self.lines[self.active].split_off(cursor);
        self.lines.insert(self.active + 1, tail);
        if self.lines.len() > BUFFER_LINES {
          // Dropping the top line shifts everything (including the inserted
          // tail) up one slot, so the tail lands back at `self.active` with
          // its cursor already at column 0.
          self.lines.remove(0);
        } else {
          self.active += 1;
        }
      }
      _ => {}
    }
  }
}

impl<P: Platform> MenuApp<P> for EditorApp<P> {
  fn render(&self) -> LcdScreen {
    let lines = self
      .lines
      .iter()
      .enumerate()
      .map(|(i, input)| TextBufferLine {
        text: input.value().to_string(),
        cursor: (i == self.active).then_some(input.cursor() as u32),
      })
      .collect();
    LcdScreen::TextBuffer { lines }
  }

  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction {
    self.ctx.update_lcd(self.render());

    loop {
      let event = ctx.next().await;
      if let Some(action) = event.exit_action() {
        return action;
      }
      match event {
        // Navigation buttons (arrows, Fire) — unified with the keyboard's arrow
        // and Enter keys, which the platform surfaces as HexButton presses.
        AppRunEvent::Input(AppInput::Button(hex)) => {
          self.handle_nav_button(hex);
          self.ctx.update_lcd(self.render());
        }
        // Character keys arrive as raw keyboard events (Tab/Escape included —
        // nav-key injection only applies to the root menu and hosted apps).
        AppRunEvent::Event(AppEvent::Device(DeviceEvent::Keyboard(ke))) => {
          self.handle_key(ke.code, ke.typ);
          self.ctx.update_lcd(self.render());
        }
        _ => {}
      }
    }
  }
}
