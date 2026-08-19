use app::platform::input::InputManager;
use app::types::HexButton;
use app::utils::EventQueue;
use std::pin::Pin;

/// Same depth as the firmware's button queue.
const EVENT_QUEUE_DEPTH: usize = 10;

type ButtonEventQueue = EventQueue<HexButton, EVENT_QUEUE_DEPTH>;

#[derive(Clone, Debug)]
pub struct DesktopInputManager {
  events: ButtonEventQueue,
}

impl DesktopInputManager {
  pub fn new() -> Self {
    Self {
      events: ButtonEventQueue::new(),
    }
  }

  /// Called from the minifb thread when a key is pressed. Clones of the manager
  /// (or this handle) share the queue, so the minifb thread keeps a clone.
  pub fn push_button(&self, button: HexButton) {
    // Drop on overflow, same as the firmware button task's `try_push`.
    let _ = self.events.try_push(button);
  }
}

impl InputManager for DesktopInputManager {
  fn next_button(&self) -> Pin<Box<dyn std::future::Future<Output = HexButton> + Send + '_>> {
    Box::pin(self.events.next())
  }

  fn inject_button(&self, button: HexButton) -> Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    Box::pin(self.events.push(button))
  }
}
