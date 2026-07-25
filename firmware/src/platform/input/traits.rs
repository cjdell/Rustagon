use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;
use core::future::Future;
use core::pin::Pin;

pub use crate::types::HexButton;

/// Input Manager trait for button press events
/// 
/// Provides async access to button presses as a queue of events.
/// All button press events are queued and preserved even if the consumer
/// falls behind - no events are lost.
pub trait InputManager: Send + Sync + fmt::Debug {
  /// Wait for the next button press
  /// Returns immediately with the next queued button event
  fn next_button(&self) -> Pin<Box<dyn Future<Output = HexButton> + Send + '_>>;
}

/// Handle to an InputManager implementation
/// 
/// This wraps a trait object and can be cloned cheaply.
#[derive(Clone)]
pub struct InputHandle {
  inner: Arc<dyn InputManager>,
}

impl InputHandle {
  /// Create a new input handle wrapping a manager implementation
  pub fn new<M: InputManager + 'static>(manager: M) -> Self {
    Self {
      inner: Arc::new(manager),
    }
  }

  /// Wait for the next button press
  pub fn next_button(&self) -> Pin<Box<dyn Future<Output = HexButton> + Send + '_>> {
    self.inner.next_button()
  }
}

impl fmt::Debug for InputHandle {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("InputHandle").finish()
  }
}
