//! A minimal VT-style terminal renderer for the SSH app.
//!
//! The badge display (`LcdScreen::TextBuffer`) shows up to 8 lines of text in a
//! centred frame. This module turns the raw byte stream from an SSH shell into
//! those lines, handling the control codes that matter for typical command-line
//! output: CR/LF/BS/TAB, and enough of the ANSI CSI parser to swallow cursor
//! movement / erase sequences (so escape garbage never reaches the screen).
//! Colour/attribute SGR sequences are parsed and discarded.

use crate::types::{HexButton, KeyCode, LcdScreen, TextBufferLine};
use alloc::{string::String, vec, vec::Vec};

/// Maximum number of lines kept in memory. Only the tail is rendered.
const MAX_LINES: usize = 64;
/// Number of lines the display can show at once.
pub const DISPLAY_LINES: usize = 8;

/// Parser state for ANSI escape sequences.
enum EscState {
  None,
  /// Just saw `ESC` — waiting for the introducer character.
  Esc,
  /// In a CSI (`ESC [ …`) sequence; params collected up to the final byte.
  Csi(Vec<u8>),
  /// In an OSC/DCS (`ESC ] …` / `ESC P …`) sequence; ignored until terminator.
  Ignore,
}

/// Minimal VT-style terminal. Owns a bounded ring of lines plus a cursor.
pub struct Terminal {
  lines: Vec<String>,
  cur_line: usize,
  /// Cursor column, in characters.
  cur_x: usize,
  esc: EscState,
  /// UTF-8 codepoint accumulator.
  utf8: [u8; 4],
  utf8_len: usize,
}

impl Default for Terminal {
  fn default() -> Self {
    Self::new()
  }
}

impl Terminal {
  pub fn new() -> Self {
    Self {
      lines: vec![String::new()],
      cur_line: 0,
      cur_x: 0,
      esc: EscState::None,
      utf8: [0; 4],
      utf8_len: 0,
    }
  }

  /// Feed a chunk of raw shell output.
  pub fn feed(&mut self, data: &[u8]) {
    for &b in data {
      self.feed_byte(b);
    }
  }

  fn feed_byte(&mut self, b: u8) {
    // ESC always starts/continues an escape sequence, never a literal byte.
    if b == 0x1b {
      self.utf8_len = 0;
      match self.esc {
        EscState::None => self.esc = EscState::Esc,
        // Doubled ESC / mid-sequence: stay put.
        EscState::Esc | EscState::Ignore => {}
        // ESC in the middle of a CSI aborts it (e.g. `ESC ESC [`).
        EscState::Csi(_) => self.esc = EscState::Ignore,
      }
      return;
    }

    // UTF-8: accumulate multi-byte codepoints.
    if b >= 0x80 || self.utf8_len > 0 {
      if self.utf8_len > 0 && (b & 0xc0) != 0x80 {
        // Broken sequence mid-way; discard and process `b` normally.
        self.utf8_len = 0;
      } else {
        self.utf8[self.utf8_len] = b;
        self.utf8_len += 1;
        let need = utf8_len(self.utf8[0]);
        if self.utf8_len == need {
          if let Ok(s) = core::str::from_utf8(&self.utf8[..need])
            && let Some(ch) = s.chars().next()
          {
            self.put_char(ch);
          }
          self.utf8_len = 0;
        }
        return;
      }
    }

    match self.esc {
      EscState::None => self.feed_plain(b),
      EscState::Esc => match b {
        b'[' => self.esc = EscState::Csi(Vec::new()),
        b']' | b'P' | b'_' | b'^' => self.esc = EscState::Ignore,
        _ => self.esc = EscState::Ignore,
      },
      EscState::Csi(ref mut buf) => {
        if buf.len() > 32 {
          self.esc = EscState::None;
          return;
        }
        // A CSI sequence ends at the first byte in the final-byte range.
        if (0x40..=0x7e).contains(&b) {
          let params = core::mem::take(buf);
          self.esc = EscState::None;
          self.handle_csi(&params, b);
        } else {
          buf.push(b);
        }
      }
      EscState::Ignore => match b {
        0x07 | 0x9c => self.esc = EscState::None,
        _ => {}
      },
    }
  }

  fn feed_plain(&mut self, b: u8) {
    match b {
      b'\r' => self.cur_x = 0,
      b'\n' => self.newline(),
      0x08 | 0x7f => self.backspace(),
      b'\t' => {
        for _ in 0..4 {
          self.put_char(' ');
        }
      }
      0x00..=0x1f => {} // other C0 controls: ignore
      _ => {
        if let Some(ch) = core::char::from_u32(b as u32) {
          self.put_char(ch);
        }
      }
    }
  }

  /// Handle a completed CSI sequence. `params` is everything between `[` and
  /// the final byte (e.g. `?25` for `ESC[?25l`).
  fn handle_csi(&mut self, params: &[u8], final_byte: u8) {
    match final_byte {
      b'J' => {
        let n = csi_param(params, 0);
        match n {
          2 | 3 => self.erase_display(),
          _ => self.erase_from_cursor(),
        }
      }
      b'K' => {
        let n = csi_param(params, 0);
        match n {
          0 => self.erase_line_from_cursor(),
          1 => self.erase_line_to_cursor(),
          _ => {
            self.lines[self.cur_line].clear();
            self.cur_x = 0;
          }
        }
      }
      b'H' | b'f' => {
        if params.is_empty() {
          self.cur_line = 0;
          self.cur_x = 0;
        } else {
          let mut nums = params.split(|&c| c == b';');
          let row = nums.next().and_then(parse_uint).unwrap_or(1).saturating_sub(1) as usize;
          let col = nums.next().and_then(parse_uint).unwrap_or(1).saturating_sub(1) as usize;
          self.cur_line = row.min(self.lines.len().saturating_sub(1));
          self.cur_x = col;
        }
      }
      _ => {} // cursor movement, SGR colour, modes, etc.: ignore
    }
  }

  fn put_char(&mut self, ch: char) {
    let line = &mut self.lines[self.cur_line];
    let count = line.chars().count();
    if self.cur_x >= count {
      line.push(ch);
      self.cur_x = count + 1;
    } else {
      let mut chars: Vec<char> = line.chars().collect();
      chars[self.cur_x] = ch;
      *line = chars.into_iter().collect();
      self.cur_x += 1;
    }
  }

  fn newline(&mut self) {
    // Discard any lines below the cursor (terminal behaviour when the cursor
    // has been moved up by a CSI sequence).
    if self.cur_line + 1 < self.lines.len() {
      self.lines.truncate(self.cur_line + 1);
    }
    self.lines.push(String::new());
    self.cur_line = self.lines.len() - 1;
    self.cur_x = 0;
    if self.lines.len() > MAX_LINES {
      let overflow = self.lines.len() - MAX_LINES;
      self.lines.drain(..overflow);
      self.cur_line -= overflow;
    }
  }

  fn backspace(&mut self) {
    let line = &mut self.lines[self.cur_line];
    if self.cur_x > 0 {
      let mut chars: Vec<char> = line.chars().collect();
      let col = self.cur_x.min(chars.len());
      chars.remove(col - 1);
      *line = chars.into_iter().collect();
      self.cur_x -= 1;
    }
  }

  fn erase_display(&mut self) {
    self.lines = vec![String::new()];
    self.cur_line = 0;
    self.cur_x = 0;
  }

  fn erase_from_cursor(&mut self) {
    self.erase_line_from_cursor();
    self.lines.truncate(self.cur_line + 1);
  }

  fn erase_line_from_cursor(&mut self) {
    let line = &mut self.lines[self.cur_line];
    let mut chars: Vec<char> = line.chars().collect();
    let col = self.cur_x.min(chars.len());
    chars.truncate(col);
    *line = chars.into_iter().collect();
  }

  fn erase_line_to_cursor(&mut self) {
    let line = &mut self.lines[self.cur_line];
    let mut chars: Vec<char> = line.chars().collect();
    let col = self.cur_x.min(chars.len());
    let rest = chars.split_off(col);
    *line = rest.into_iter().collect();
    self.cur_x = 0;
  }

  /// Render the terminal as a `TextBuffer` screen. Passes a bottom-aligned
  /// window of up to [`DISPLAY_LINES`] lines (the display frame shows 8).
  pub fn render(&self) -> LcdScreen {
    let window = self.lines.len().min(DISPLAY_LINES);
    let start = self.lines.len() - window;
    let mut lines = Vec::with_capacity(DISPLAY_LINES);
    for _ in 0..(DISPLAY_LINES - window) {
      lines.push(TextBufferLine {
        text: String::new(),
        cursor: None,
      });
    }
    for i in start..self.lines.len() {
      let cursor = if i == self.cur_line {
        Some(self.char_to_byte(i, self.cur_x) as u32)
      } else {
        None
      };
      lines.push(TextBufferLine {
        text: self.lines[i].clone(),
        cursor,
      });
    }
    LcdScreen::TextBuffer { lines }
  }

  /// Convert a character column to a byte index into the given line.
  fn char_to_byte(&self, line_idx: usize, col: usize) -> usize {
    match self.lines.get(line_idx) {
      Some(line) => line.char_indices().nth(col).map(|(i, _)| i).unwrap_or_else(|| line.len()),
      None => 0,
    }
  }
}

fn utf8_len(lead: u8) -> usize {
  if lead < 0x80 {
    1
  } else if lead < 0xe0 {
    2
  } else if lead < 0xf0 {
    3
  } else {
    4
  }
}

/// First numeric parameter of a CSI sequence (0 when absent).
fn csi_param(params: &[u8], default: u32) -> u32 {
  let first = params.split(|&c| c == b';').next().unwrap_or_default();
  if first.first() == Some(&b'?') {
    parse_uint(&first[1..]).unwrap_or(default)
  } else {
    parse_uint(first).unwrap_or(default)
  }
}

fn parse_uint(bytes: &[u8]) -> Option<u32> {
  if bytes.is_empty() {
    return None;
  }
  let mut n: u32 = 0;
  for &b in bytes {
    if !b.is_ascii_digit() {
      return None;
    }
    n = n.saturating_mul(10).saturating_add((b - b'0') as u32);
  }
  Some(n)
}

/// Map a keyboard key to the byte sequence to send to the remote shell.
///
/// Character keys produce their UTF-8 bytes (Shift-aware); editing keys produce
/// their control sequences. Directional keys and Enter never arrive here — the
/// platform unifies them into [`HexButton`] presses, so they are handled by
/// [`hex_button_to_bytes`].
pub fn key_to_bytes(code: KeyCode, shifted: bool) -> Option<Vec<u8>> {
  match code {
    KeyCode::Enter => Some(vec![b'\r']),
    KeyCode::Backspace => Some(vec![0x7f]),
    KeyCode::Tab => Some(vec![0x09]),
    KeyCode::Escape => Some(vec![0x1b]),
    _ => code.to_char(shifted).map(|ch| {
      let mut buf = [0u8; 4];
      let s = ch.encode_utf8(&mut buf);
      s.as_bytes().to_vec()
    }),
  }
}

/// Map a physical hex button to the byte sequence to send to the remote shell.
pub fn hex_button_to_bytes(button: HexButton) -> Option<Vec<u8>> {
  match button {
    HexButton::Up => Some(b"\x1b[A".to_vec()),
    HexButton::Down => Some(b"\x1b[B".to_vec()),
    HexButton::Right => Some(b"\x1b[C".to_vec()),
    HexButton::Left => Some(b"\x1b[D".to_vec()),
    HexButton::Fire => Some(b"\r".to_vec()),
    // Ctrl-characters on the function ring (like a mini keyboard).
    HexButton::HexA => Some(b"\x01".to_vec()), // Ctrl-A (line start)
    HexButton::HexB => Some(b"\x02".to_vec()), // Ctrl-B (back one char)
    HexButton::HexC => Some(b"\x03".to_vec()), // Ctrl-C (interrupt)
    HexButton::HexD => Some(b"\x04".to_vec()), // Ctrl-D (EOF)
    HexButton::HexE => Some(b"\x05".to_vec()), // Ctrl-E (line end)
    HexButton::HexF => Some(b"\x1b".to_vec()), // Escape
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn collect(screen: LcdScreen) -> Vec<String> {
    match screen {
      LcdScreen::TextBuffer { lines } => lines.into_iter().map(|l| l.text).collect(),
      _ => unreachable!(),
    }
  }

  #[test]
  fn plain_text_goes_to_first_line() {
    let mut t = Terminal::new();
    t.feed(b"hello");
    assert_eq!(t.lines[0], "hello");
    assert_eq!(t.cur_x, 5);
  }

  #[test]
  fn newline_advances() {
    let mut t = Terminal::new();
    t.feed(b"abc\r\ndef\r\n");
    assert_eq!(t.lines[0], "abc");
    assert_eq!(t.lines[1], "def");
    assert_eq!(t.lines[2], "");
    assert_eq!(t.cur_line, 2);
  }

  #[test]
  fn cr_overwrites_in_place() {
    let mut t = Terminal::new();
    t.feed(b"12345\rXY");
    assert_eq!(t.lines[0], "XY345");
    assert_eq!(t.cur_x, 2);
  }

  #[test]
  fn backspace_removes() {
    let mut t = Terminal::new();
    t.feed(b"abc\x7f");
    assert_eq!(t.lines[0], "ab");
  }

  #[test]
  fn csi_sequences_are_swallowed() {
    let mut t = Terminal::new();
    t.feed(b"\x1b[31mred\x1b[0m");
    assert_eq!(t.lines[0], "red");
    t.feed(b"\x1b[2J");
    assert!(t.lines.len() == 1 && t.lines[0].is_empty());
  }

  #[test]
  fn carriage_return_after_clear() {
    let mut t = Terminal::new();
    t.feed(b"a\x1b[H\x1b[2Jb");
    assert_eq!(collect(t.render()).last().unwrap(), "b");
  }

  #[test]
  fn scroll_bounds_length() {
    let mut t = Terminal::new();
    for _ in 0..100 {
      t.feed(b"line\r\n");
    }
    assert!(t.lines.len() <= MAX_LINES);
    assert_eq!(collect(t.render()).len(), DISPLAY_LINES);
  }
}
