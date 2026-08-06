use alloc::sync::Arc;
use core::fmt;
use display_types::LcdScreen;

#[derive(Debug, Clone)]
pub enum DisplayError {
  SignalBusy,
  /// The submitted raw frame was not `DISPLAY_WIDTH * DISPLAY_HEIGHT * 2` bytes.
  InvalidFrame,
}

pub const DISPLAY_WIDTH: usize = 240;
pub const DISPLAY_HEIGHT: usize = 240;

/// Byte length of one raw RGB565 frame.
pub const FRAME_BYTES: usize = DISPLAY_WIDTH * DISPLAY_HEIGHT * 2;

pub trait DisplayManager: Send + Sync + fmt::Debug {
  fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError>;
  fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError>;

  /// Returns a byte-slice view of the current frame buffer (raw RGB565 pixels).
  /// The slice length is `DISPLAY_WIDTH * DISPLAY_HEIGHT * 2` bytes when `Some`.
  ///
  /// The buffer may be concurrently mutated by the display rendering task — callers
  /// should treat the returned data as a best-effort snapshot.
  fn frame_buffer(&self) -> Option<&[u8]>;

  /// Submit a raw RGB565 framebuffer (`FRAME_BYTES` bytes) for display.
  ///
  /// Non-blocking: implementations copy the frame and flush it to the physical
  /// display asynchronously (e.g. on a dedicated display task), so callers can
  /// immediately start rendering the next frame. Returns
  /// [`DisplayError::InvalidFrame`] if `buffer` is not exactly `FRAME_BYTES` long.
  fn signal_raw_frame(&self, buffer: &[u8]) -> Result<(), DisplayError>;
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

  pub fn signal_raw_frame(&self, buffer: &[u8]) -> Result<(), DisplayError> {
    self.inner.signal_raw_frame(buffer)
  }
}
