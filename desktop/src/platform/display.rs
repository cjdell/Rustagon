use app::platform::display::{DisplayError, DisplayManager};
use core::fmt;
use display_types::LcdScreen;
use std::sync::Mutex;

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
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
