use app::platform::input::InputManager;
use app::types::HexButton;
use core::fmt;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use std::pin::Pin;

static BUTTON_SIGNAL: Signal<CriticalSectionRawMutex, HexButton> = Signal::new();

#[derive(Debug)]
pub struct DesktopInputManager;

impl DesktopInputManager {
    pub fn new() -> Self { Self }

    /// Called from the minifb thread when a key is pressed.
    pub fn push_button(button: HexButton) {
        BUTTON_SIGNAL.signal(button);
    }
}

impl InputManager for DesktopInputManager {
    fn next_button(&self) -> Pin<Box<dyn std::future::Future<Output = HexButton> + Send + '_>> {
        Box::pin(async { BUTTON_SIGNAL.wait().await })
    }

    fn inject_button(&self, button: HexButton) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(async move { BUTTON_SIGNAL.signal(button); })
    }
}
