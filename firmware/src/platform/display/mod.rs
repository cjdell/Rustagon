pub mod common;
pub mod hardware;
pub mod mock;
pub mod traits;

pub use hardware::{HardwareDisplayManager, BUFFER, lcd_task, SPI_DISPLAY_INTERFACE};
pub use mock::MockDisplayManager;
pub use traits::{DisplayError, DisplayHandle, DisplayManager, LcdSignal};
