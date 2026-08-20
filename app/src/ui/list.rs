//! A scrolling list: items plus the selected index.
//!
//! Windowing/scrolling is the renderer's job (`LcdScreen::Menu` draws the
//! list around `selected`); the widget tracks the selection and movement, so
//! any app that renders "a list with a cursor" (root menu, pickers) can share
//! the same navigation code.

use alloc::vec::Vec;

/// Items plus the currently selected index.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct List<T> {
  items: Vec<T>,
  /// Index of the selected item (0 when the list is empty).
  selected: usize,
}

impl<T> List<T> {
  pub fn new(items: Vec<T>) -> Self {
    Self { items, selected: 0 }
  }

  pub fn len(&self) -> usize {
    self.items.len()
  }

  pub fn is_empty(&self) -> bool {
    self.items.is_empty()
  }

  /// Borrow the items.
  pub fn items(&self) -> &Vec<T> {
    &self.items
  }

  pub fn get(&self, index: usize) -> Option<&T> {
    self.items.get(index)
  }

  /// The currently selected index (always < `len()`, or 0 when empty).
  pub fn selected(&self) -> usize {
    self.selected
  }

  /// The currently selected item, if the list is non-empty.
  pub fn selected_item(&self) -> Option<&T> {
    self.items.get(self.selected)
  }

  /// Select an index, clamped into range.
  pub fn set_selected(&mut self, index: usize) {
    self.selected = index.min(self.items.len().saturating_sub(1));
  }

  /// Move the selection up one (clamped at the first item).
  pub fn move_up(&mut self) {
    if self.selected > 0 {
      self.selected -= 1;
    }
  }

  /// Move the selection down one (clamped at the last item).
  pub fn move_down(&mut self) {
    if self.selected < self.items.len().saturating_sub(1) {
      self.selected += 1;
    }
  }

  /// Replace an item in place (e.g. a refresh that must keep the selection).
  pub fn set_item(&mut self, index: usize, item: T) {
    if let Some(slot) = self.items.get_mut(index) {
      *slot = item;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use alloc::vec;

  #[test]
  fn movement_clamps_at_both_ends() {
    let mut l = List::new(vec![0, 1, 2]);
    l.move_up();
    assert_eq!(l.selected(), 0);
    l.move_down();
    assert_eq!(l.selected(), 1);
    l.move_down();
    assert_eq!(l.selected(), 2);
    l.move_down();
    assert_eq!(l.selected(), 2);
    l.move_up();
    assert_eq!(l.selected(), 1);
    assert_eq!(l.selected_item(), Some(&1));
  }

  #[test]
  fn empty_list_stays_at_zero() {
    let mut l: List<u8> = List::new(Vec::new());
    assert_eq!(l.selected(), 0);
    assert_eq!(l.selected_item(), None);
    l.move_up();
    l.move_down();
    assert_eq!(l.selected(), 0);
  }

  #[test]
  fn set_selected_clamps() {
    let mut l = List::new(vec!['a', 'b']);
    l.set_selected(9);
    assert_eq!(l.selected(), 1);
    l.set_selected(0);
    assert_eq!(l.selected(), 0);
  }

  #[test]
  fn set_item_keeps_selection() {
    let mut l = List::new(vec!["a", "b"]);
    l.move_down();
    l.set_item(1, "B");
    assert_eq!(l.selected(), 1);
    assert_eq!(l.selected_item(), Some(&"B"));
  }
}
