pub use app::platform::display::{DisplayError, DisplayHandle, DisplayManager};

use crate::types::LcdScreen;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

pub type LcdSignal = Signal<CriticalSectionRawMutex, LcdScreen>;
