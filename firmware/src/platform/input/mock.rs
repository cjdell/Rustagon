use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::fmt;
use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  rwlock::RwLock,
};

use super::traits::*;
use crate::utils::WatchedValue;

/// Mock Input Manager for testing
/// 
/// Simulates button presses for testing without hardware
#[derive(Clone)]
pub struct MockInputManager {
  pending_buttons: Arc<RwLock<CriticalSectionRawMutex, VecDeque<HexButton>>>,
  button_signal: Arc<WatchedValue<()>>,
}

impl MockInputManager {
  pub fn new() -> Self {
    Self {
      pending_buttons: Arc::new(RwLock::new(VecDeque::new())),
      button_signal: Arc::new(WatchedValue::new(())),
    }
  }

  /// Queue a button press for testing
  pub async fn queue_button(&self, button: HexButton) {
    self.pending_buttons.write().await.push_back(button);
    // Notify any waiting consumers
    self.button_signal.set(()).await;
  }
}

impl fmt::Debug for MockInputManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("MockInputManager").finish()
  }
}

impl InputManager for MockInputManager {
  fn next_button(&self) -> core::pin::Pin<Box<dyn core::future::Future<Output = HexButton> + Send + '_>> {
    let buttons = self.pending_buttons.clone();
    let signal = self.button_signal.clone();
    
    Box::pin(async move {
      loop {
        // Check if there are pending buttons
        if let Some(button) = buttons.write().await.pop_front() {
          return button;
        }
        // Wait for a button to be queued
        signal.wait_for_change().await;
      }
    })
  }
}
