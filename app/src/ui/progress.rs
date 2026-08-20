//! A bounded progress bar: `done` / `total`, rendered as
//! `LcdScreen::BoundedProgress` (the display draws the bar; this widget is
//! the shared state + render helper for download/OTA progress).

use display_types::LcdScreen;

/// A bounded progress value (e.g. bytes downloaded of the file size).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
  done: u32,
  total: u32,
}

impl Progress {
  pub fn new(done: u32, total: u32) -> Self {
    Self { done, total }
  }

  pub fn done(&self) -> u32 {
    self.done
  }

  pub fn total(&self) -> u32 {
    self.total
  }

  /// Set the completed amount (clamped to `total` for display).
  pub fn set_done(&mut self, done: u32) {
    self.done = done.min(self.total);
  }

  /// True when `done >= total`.
  pub fn complete(&self) -> bool {
    self.done >= self.total
  }

  /// The progress bar screen.
  pub fn render(&self) -> LcdScreen {
    LcdScreen::BoundedProgress(self.done, self.total)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn renders_bounded_progress() {
    let p = Progress::new(512, 4096);
    match p.render() {
      LcdScreen::BoundedProgress(d, t) => {
        assert_eq!(d, 512);
        assert_eq!(t, 4096);
      }
      _ => panic!("expected BoundedProgress"),
    }
    assert!(!p.complete());
  }

  #[test]
  fn set_done_clamps_to_total() {
    let mut p = Progress::new(0, 100);
    p.set_done(250);
    assert_eq!(p.done(), 100);
    assert!(p.complete());
  }
}
