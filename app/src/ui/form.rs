//! A small labelled form: text fields plus an action row, with focus
//! navigation (Up/Down move between rows, Fire activates the action).
//!
//! The SSH connect screen (host/user/key/port + `[Connect]`) is the current
//! consumer; future settings/confirm dialogs can reuse it.

use super::text_input::TextInput;
use alloc::{format, string::String, vec::Vec};
use display_types::TextBufferLine;

/// One labelled field in a [`Form`].
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
  label: &'static str,
  /// The field's editable text state.
  pub input: TextInput,
}

impl Field {
  /// A field pre-filled with `value` (cursor at the end).
  pub fn new(label: &'static str, value: &str) -> Self {
    Self {
      label,
      input: TextInput::with_value(value),
    }
  }

  pub fn label(&self) -> &'static str {
    self.label
  }

  pub fn value(&self) -> &str {
    self.input.value()
  }

  /// The field's cursor (byte index into [`Field::value`]).
  pub fn cursor(&self) -> usize {
    self.input.cursor()
  }
}

/// Labelled fields plus a final action row. The active row is a field
/// (indices `0..fields().len()`) or the action row (`fields().len()`).
#[derive(Clone, Debug, PartialEq)]
pub struct Form {
  fields: Vec<Field>,
  action_label: String,
  active: usize,
}

impl Form {
  /// A form with the given fields and action-row label (e.g. `"[Connect]"`).
  /// The first field is active.
  pub fn new(fields: Vec<Field>, action_label: impl Into<String>) -> Self {
    Self {
      fields,
      action_label: action_label.into(),
      active: 0,
    }
  }

  pub fn fields(&self) -> &Vec<Field> {
    &self.fields
  }

  /// Number of navigable rows: one per field, plus the action row.
  pub fn row_count(&self) -> usize {
    self.fields.len() + 1
  }

  /// The index of the active row (into [`Form::row_count`]).
  pub fn active(&self) -> usize {
    self.active
  }

  pub fn field(&self, index: usize) -> Option<&Field> {
    self.fields.get(index)
  }

  /// The field under the cursor, if the active row is a field.
  pub fn active_field(&self) -> Option<&Field> {
    self.fields.get(self.active)
  }

  /// The field under the cursor, if the active row is a field.
  pub fn active_field_mut(&mut self) -> Option<&mut Field> {
    self.fields.get_mut(self.active)
  }

  /// True when the active row is the action row (Fire should activate it).
  pub fn active_is_action(&self) -> bool {
    self.active == self.fields.len()
  }

  /// Move focus up one row (clamped at the first field).
  pub fn up(&mut self) {
    if self.active > 0 {
      self.active -= 1;
    }
  }

  /// Move focus down one row (clamped at the action row).
  pub fn down(&mut self) {
    if self.active < self.row_count() - 1 {
      self.active += 1;
    }
  }

  /// Render the field + action rows. The active field carries the text
  /// cursor; the action row carries a cursor at column 0 when active.
  pub fn field_lines(&self) -> Vec<TextBufferLine> {
    let mut lines = Vec::with_capacity(self.row_count());
    for (i, f) in self.fields.iter().enumerate() {
      lines.push(TextBufferLine {
        text: format!("{}: {}", f.label, f.value()),
        cursor: (i == self.active).then_some(f.cursor() as u32),
      });
    }
    lines.push(TextBufferLine {
      text: self.action_label.clone(),
      cursor: self.active_is_action().then_some(0),
    });
    lines
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use alloc::vec;

  fn form() -> Form {
    Form::new(
      vec![Field::new("host", "1.2.3.4"), Field::new("user", "me"), Field::new("port", "22")],
      "[Connect]",
    )
  }

  #[test]
  fn focus_navigates_fields_then_action() {
    let mut f = form();
    assert_eq!(f.active(), 0);
    assert!(!f.active_is_action());
    f.down();
    assert_eq!(f.active(), 1);
    f.down();
    assert_eq!(f.active(), 2);
    f.down();
    assert!(f.active_is_action());
    f.down(); // clamped at the action row
    assert_eq!(f.active(), 3);
    assert_eq!(f.row_count(), 4);
    f.up();
    assert_eq!(f.active(), 2);
    f.up();
    f.up();
    f.up(); // clamped at the first field
    assert_eq!(f.active(), 0);
  }

  #[test]
  fn field_values_and_accessors() {
    let f = form();
    assert_eq!(f.field(0).unwrap().value(), "1.2.3.4");
    assert_eq!(f.field(2).unwrap().value(), "22");
    assert_eq!(f.field(3), None);
    assert_eq!(f.active_field().unwrap().label(), "host");
  }

  #[test]
  fn field_lines_mark_active_cursor() {
    let mut f = form();
    let lines = f.field_lines();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].text, "host: 1.2.3.4");
    assert_eq!(lines[0].cursor, Some(7)); // "1.2.3.4"
    assert_eq!(lines[1].cursor, None);
    assert_eq!(lines[3].text, "[Connect]");
    assert_eq!(lines[3].cursor, None);

    f.down();
    f.down();
    f.down(); // action row
    let lines = f.field_lines();
    assert_eq!(lines[0].cursor, None);
    assert_eq!(lines[3].cursor, Some(0));
  }

  #[test]
  fn active_field_mut_edits_value() {
    let mut f = form();
    if let Some(field) = f.active_field_mut() {
      field.input.backspace();
    }
    assert_eq!(f.field(0).unwrap().value(), "1.2.3.");
  }
}
