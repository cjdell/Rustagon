//! A single-line text input: a mutable `String` plus a byte-index cursor.
//!
//! The cursor is a *byte* index into the value (matching
//! [`TextBufferLine::cursor`]), and it is always kept on a UTF-8 character
//! boundary. Movement is character-aware (Left/Right never land mid-char),
//! which is identical to byte movement for the ASCII input the badge
//! keyboard produces.

use alloc::string::{String, ToString};

/// A single-line text input with an insert/delete cursor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextInput {
  value: String,
  cursor: usize,
}

impl TextInput {
  /// An empty input with the cursor at the start (which is also the end).
  pub fn new() -> Self {
    Self {
      value: String::new(),
      cursor: 0,
    }
  }

  /// An input pre-filled with `value`; the cursor is placed at the end.
  pub fn with_value(value: &str) -> Self {
    let mut s = Self::new();
    s.set_value(value);
    s
  }

  /// Replace the value, placing the cursor at the end.
  pub fn set_value(&mut self, value: &str) {
    self.value = value.to_string();
    self.cursor = self.value.len();
  }

  pub fn value(&self) -> &str {
    &self.value
  }

  pub fn len(&self) -> usize {
    self.value.len()
  }

  pub fn is_empty(&self) -> bool {
    self.value.is_empty()
  }

  /// The cursor position as a byte index (render with
  /// `TextBufferLine::cursor`).
  pub fn cursor(&self) -> usize {
    self.cursor
  }

  pub fn cursor_at_start(&self) -> bool {
    self.cursor == 0
  }

  pub fn cursor_at_end(&self) -> bool {
    self.cursor == self.value.len()
  }

  /// Insert a character at the cursor, advancing the cursor.
  pub fn insert_char(&mut self, ch: char) {
    self.value.insert(self.cursor, ch);
    self.cursor += ch.len_utf8();
  }

  /// Insert a string at the cursor, advancing the cursor.
  pub fn insert_str(&mut self, s: &str) {
    self.value.insert_str(self.cursor, s);
    self.cursor += s.len();
  }

  /// Append a character at the end and place the cursor there. This is the
  /// "append-only field" mode used by forms where the user cannot move the
  /// cursor: backspace then always removes the last character.
  pub fn push_char(&mut self, ch: char) {
    self.value.push(ch);
    self.cursor = self.value.len();
  }

  /// Remove the character before the cursor (Backspace).
  pub fn backspace(&mut self) {
    if self.cursor > 0 {
      let start = self.value.floor_char_boundary(self.cursor - 1);
      self.value.replace_range(start..self.cursor, "");
      self.cursor = start;
    }
  }

  /// Remove the character at the cursor (Delete).
  pub fn delete(&mut self) {
    if self.cursor < self.value.len() {
      let end = self.value.ceil_char_boundary(self.cursor + 1);
      self.value.replace_range(self.cursor..end, "");
    }
  }

  /// Move the cursor to the start of the line (Home).
  pub fn home(&mut self) {
    self.cursor = 0;
  }

  /// Move the cursor to the end of the line (End).
  pub fn end(&mut self) {
    self.cursor = self.value.len();
  }

  /// Move the cursor one character left (clamped at the start).
  pub fn left(&mut self) {
    if self.cursor > 0 {
      self.cursor = self.value.floor_char_boundary(self.cursor - 1);
    }
  }

  /// Move the cursor one character right (clamped at the end).
  pub fn right(&mut self) {
    if self.cursor < self.value.len() {
      self.cursor = self.value.ceil_char_boundary(self.cursor + 1);
    }
  }

  /// Split the value at the cursor: the tail becomes a new [`TextInput`]
  /// (cursor at its start) and this input keeps the head (cursor at the
  /// split point). Used by the Editor for Enter.
  pub fn split_off(&mut self, at: usize) -> TextInput {
    let at = at.min(self.value.len());
    let tail = self.value.split_off(at);
    self.cursor = at;
    Self { value: tail, cursor: 0 }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn new_is_empty() {
    let t = TextInput::new();
    assert_eq!(t.value(), "");
    assert_eq!(t.cursor(), 0);
    assert!(t.cursor_at_start() && t.cursor_at_end());
  }

  #[test]
  fn with_value_puts_cursor_at_end() {
    let t = TextInput::with_value("abc");
    assert_eq!(t.value(), "abc");
    assert_eq!(t.cursor(), 3);
  }

  #[test]
  fn insert_and_backspace() {
    let mut t = TextInput::new();
    t.insert_char('h');
    t.insert_char('i');
    assert_eq!(t.value(), "hi");
    assert_eq!(t.cursor(), 2);
    t.backspace();
    assert_eq!(t.value(), "h");
    assert_eq!(t.cursor(), 1);
    t.backspace();
    t.backspace(); // empty: no-op
    assert_eq!(t.value(), "");
    assert_eq!(t.cursor(), 0);
  }

  #[test]
  fn delete_removes_at_cursor() {
    let mut t = TextInput::with_value("hix");
    t.home();
    t.delete();
    assert_eq!(t.value(), "ix");
    t.delete();
    assert_eq!(t.value(), "x");
    t.end();
    t.delete(); // at end: no-op
    assert_eq!(t.value(), "x");
    assert_eq!(t.cursor(), 1);
  }

  #[test]
  fn home_end() {
    let mut t = TextInput::with_value("hello");
    t.home();
    assert_eq!(t.cursor(), 0);
    t.end();
    assert_eq!(t.cursor(), 5);
  }

  #[test]
  fn left_right_clamp() {
    let mut t = TextInput::with_value("abc");
    t.left();
    assert_eq!(t.cursor(), 2);
    t.left();
    t.left();
    t.left(); // clamped at 0
    assert_eq!(t.cursor(), 0);
    t.right();
    assert_eq!(t.cursor(), 1);
    t.end();
    t.right(); // clamped at end
    assert_eq!(t.cursor(), 3);
  }

  #[test]
  fn multibyte_movement_stays_on_boundaries() {
    let mut t = TextInput::with_value("éé"); // 2 bytes each
    t.end();
    t.left();
    assert_eq!(t.cursor(), 2);
    t.backspace();
    assert_eq!(t.value(), "é");
    t.left();
    assert_eq!(t.cursor(), 0);
    t.delete();
    assert_eq!(t.value(), "");
  }

  #[test]
  fn multibyte_mixed_movement() {
    let mut t = TextInput::with_value("aé"); // 1 + 2 bytes
    t.end();
    assert_eq!(t.cursor(), 3);
    t.left(); // back past the 2-byte é
    assert_eq!(t.cursor(), 1);
    t.left();
    assert_eq!(t.cursor(), 0);
    t.right(); // one full char (a)
    assert_eq!(t.cursor(), 1);
    t.home();
    t.delete(); // removes the ASCII 'a'
    assert_eq!(t.value(), "é");
    t.end();
    t.backspace(); // removes the 2-byte char
    assert_eq!(t.value(), "");
  }

  #[test]
  fn set_value_clamps_cursor() {
    let mut t = TextInput::with_value("abcdef");
    t.set_value("ab");
    assert_eq!(t.cursor(), 2);
    t.set_value("");
    t.backspace();
    assert_eq!(t.value(), "");
  }

  #[test]
  fn split_off_splits_at_cursor() {
    let mut t = TextInput::with_value("hello world");
    t.home();
    for _ in 0..5 {
      t.right();
    }
    let tail = t.split_off(t.cursor());
    assert_eq!(t.value(), "hello");
    assert_eq!(t.cursor(), 5);
    assert_eq!(tail.value(), " world");
    assert_eq!(tail.cursor(), 0);
  }

  #[test]
  fn push_char_appends_at_end_and_moves_cursor() {
    let mut t = TextInput::with_value("abc");
    t.push_char('d');
    assert_eq!(t.value(), "abcd");
    assert_eq!(t.cursor(), 4);
    t.backspace(); // removes the last char, like the old cursor-less fields
    assert_eq!(t.value(), "abc");
    assert_eq!(t.cursor(), 3);
  }

  #[test]
  fn insert_str_advances_by_bytes() {
    let mut t = TextInput::new();
    t.insert_str("  ");
    assert_eq!(t.value(), "  ");
    assert_eq!(t.cursor(), 2);
  }
}
