use core::future::Future;
use core::pin::Pin;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use alloc::sync::Arc;
use alloc::boxed::Box;
use embassy_sync::rwlock::RwLock;

/// A watched value that can be:
/// - Read synchronously at any time
/// - Awaited for changes asynchronously
/// - Updated and broadcast to all watchers
///
/// This is the primitive for manager state that needs both sync reads and async notifications.
pub struct WatchedValue<T: Clone + Send + Sync> {
  value: Arc<RwLock<CriticalSectionRawMutex, T>>,
  change_signal: Arc<Signal<CriticalSectionRawMutex, ()>>,
}

impl<T: Clone + Send + Sync> WatchedValue<T> {
  /// Create a new watched value with an initial value
  pub fn new(initial: T) -> Self {
    Self {
      value: Arc::new(RwLock::new(initial)),
      change_signal: Arc::new(Signal::new()),
    }
  }

  /// Get the current value (async to be compatible with async context)
  pub async fn get(&self) -> T {
    self.value.read().await.clone()
  }

  /// Set a new value and notify all waiters
  pub async fn set(&self, new_value: T) {
    *self.value.write().await = new_value;
    self.change_signal.signal(());
  }

  /// Wait for the next change to the value
  /// Returns the new value after change
  pub fn wait_for_change(&self) -> Pin<Box<dyn Future<Output = T> + Send + '_>> {
    let value = self.value.clone();
    let signal = self.change_signal.clone();
    
    Box::pin(async move {
      signal.wait().await;
      value.read().await.clone()
    })
  }
}

impl<T: Clone + Send + Sync> Clone for WatchedValue<T> {
  fn clone(&self) -> Self {
    Self {
      value: self.value.clone(),
      change_signal: self.change_signal.clone(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_watched_value_clone() {
    // Ensure it's Clone-able
    let wv = WatchedValue::new(42);
    let _wv2 = wv.clone();
  }
}
