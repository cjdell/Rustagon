use alloc::boxed::Box;
use core::fmt;

use super::traits::*;
use crate::utils::EventQueue;

const EVENT_QUEUE_DEPTH: usize = 10;

type ButtonEventQueue = EventQueue<HexButton, EVENT_QUEUE_DEPTH>;

/// Mock Input Manager for testing
///
/// Simulates button presses for testing without hardware
#[derive(Clone)]
pub struct MockInputManager {
  events: ButtonEventQueue,
}

impl MockInputManager {
  pub fn new() -> Self {
    Self {
      events: ButtonEventQueue::new(),
    }
  }

  /// Queue a button press for testing
  pub async fn queue_button(&self, button: HexButton) {
    self.events.push(button).await;
  }
}

impl Default for MockInputManager {
  fn default() -> Self {
    Self::new()
  }
}

impl fmt::Debug for MockInputManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockInputManager").finish()
  }
}

impl InputManager for MockInputManager {
  fn next_button(&self) -> core::pin::Pin<Box<dyn core::future::Future<Output = HexButton> + Send + '_>> {
    Box::pin(self.events.next())
  }

  fn inject_button(&self, button: HexButton) -> core::pin::Pin<Box<dyn core::future::Future<Output = ()> + Send + '_>> {
    Box::pin(self.events.push(button))
  }
}
