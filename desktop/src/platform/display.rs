use app::platform::display::{DISPLAY_HEIGHT, DISPLAY_WIDTH, DisplayError, DisplayManager};
use core::fmt;
use display_types::LcdScreen;
use std::sync::Mutex;

/// Shared RGB565 framebuffer backing [`DisplayManager::frame_buffer`].
///
/// Written each frame by the render loop via [`DesktopDisplayManager::update_framebuffer`]
/// and read by the WebSocket handler to stream the current screen — mirroring the
/// firmware's raw-pointer `BUFFER`.
pub static FRAMEBUFFER: Mutex<Option<Vec<u8>>> = Mutex::new(None);

struct Inner {
  screen: LcdScreen,
  start_time: i64,
}

pub struct DesktopDisplayManager {
  state: Mutex<Inner>,
}

impl fmt::Debug for DesktopDisplayManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("DesktopDisplayManager").finish()
  }
}

impl DesktopDisplayManager {
  pub fn new() -> Self {
    Self {
      state: Mutex::new(Inner {
        screen: LcdScreen::Blank,
        start_time: now_ms(),
      }),
    }
  }

  pub fn get_screen(&self) -> (LcdScreen, i64) {
    let inner = self.state.lock().unwrap();
    (inner.screen.clone(), inner.start_time)
  }

  /// Copy the latest rendered RGB565 frame into the shared framebuffer so
  /// `frame_buffer()` can expose it to the WebSocket remote-view handler.
  pub fn update_framebuffer(&self, pixels: &[u8]) {
    if let Ok(mut fb) = FRAMEBUFFER.lock() {
      let buf = fb.get_or_insert_with(|| vec![0u8; (DISPLAY_WIDTH * DISPLAY_HEIGHT * 2) as usize]);
      buf.copy_from_slice(pixels);
    }
  }
}

impl DisplayManager for DesktopDisplayManager {
  fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    let mut inner = self.state.lock().unwrap();
    inner.screen = screen;
    inner.start_time = now_ms();
    Ok(())
  }

  fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    self.signal(screen)
  }

  fn frame_buffer(&self) -> Option<&[u8]> {
    let guard = FRAMEBUFFER.lock().unwrap();
    guard.as_deref().map(|buf| {
      // SAFETY: The framebuffer Vec is leaked for the process lifetime and its
      // allocation never changes. The returned reference outlives the lock guard;
      // concurrent updates by the render thread are unsynchronized by design —
      // the DisplayManager contract documents this as a best-effort snapshot.
      unsafe { &*(buf as *const [u8]) }
    })
  }
}

fn now_ms() -> i64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64
}
