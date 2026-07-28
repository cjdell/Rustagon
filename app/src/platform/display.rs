use alloc::sync::Arc;
use core::fmt;
use display_types::LcdScreen;

#[derive(Debug, Clone)]
pub enum DisplayError {
  SignalBusy,
}

pub const DISPLAY_WIDTH: usize = 240;
pub const DISPLAY_HEIGHT: usize = 240;

pub trait DisplayManager: Send + Sync + fmt::Debug {
  fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError>;
  fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError>;

  /// Returns a byte-slice view of the current frame buffer (raw RGB565 pixels).
  /// The slice length is `DISPLAY_WIDTH * DISPLAY_HEIGHT * 2` bytes when `Some`.
  ///
  /// The buffer may be concurrently mutated by the display rendering task — callers
  /// should treat the returned data as a best-effort snapshot.
  fn frame_buffer(&self) -> Option<&[u8]>;
}

#[derive(Clone, Debug)]
pub struct DisplayHandle {
  inner: Arc<dyn DisplayManager>,
}

impl DisplayHandle {
  pub fn new(manager: Arc<dyn DisplayManager>) -> Self {
    Self { inner: manager }
  }

  pub fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    self.inner.signal(screen)
  }

  pub fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    self.inner.try_signal(screen)
  }

  pub fn frame_buffer(&self) -> Option<&[u8]> {
    self.inner.frame_buffer()
  }
}
