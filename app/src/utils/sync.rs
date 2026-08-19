//! Platform-agnostic sync primitives for manager state and events.
//!
//! - [`WatchedValue<T>`] - for *state* (latest value wins, late readers see the current value)
//! - [`EventQueue<T, N>`] - for *events* (every occurrence matters, nothing is lost)
//!
//! Both are backed by `embassy-sync` and therefore work on every host (Embassy
//! on firmware, std futures on desktop).

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;
use core::future::Future;
use core::pin::Pin;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::rwlock::RwLock;
use embassy_sync::signal::Signal;

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

impl<T: Clone + Send + Sync> fmt::Debug for WatchedValue<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("WatchedValue").finish()
  }
}

/// A cloneable, heap-allocated event queue for manager -> application event delivery.
///
/// This is the counterpart to [`WatchedValue`]:
///
/// - `WatchedValue<T>` - for *state* (latest value wins, late readers see current value)
/// - `EventQueue<T, N>` - for *events* (every occurrence matters, nothing is lost)
///
/// Producers (background tasks) call [`push`](Self::push) / [`try_push`](Self::try_push),
/// consumers call [`next`](Self::next) which suspends until an event arrives. There is no
/// polling and no timers: the underlying Embassy channel wakes the consumer directly.
///
/// Unlike a raw `Channel`, this owns its storage via `Arc`, so managers do not need a
/// `static` channel or to have `Sender`/`Receiver` passed in from `main`.
///
/// Note: a single event is delivered to exactly one consumer (it is a queue, not a
/// broadcast). If several consumers await concurrently, one of them gets each event.
pub struct EventQueue<T, const N: usize> {
  channel: Arc<Channel<CriticalSectionRawMutex, T, N>>,
}

impl<T, const N: usize> EventQueue<T, N> {
  pub fn new() -> Self {
    Self {
      channel: Arc::new(Channel::new()),
    }
  }

  /// Wait for the next event. Suspends without polling until one is available.
  pub async fn next(&self) -> T {
    self.channel.receive().await
  }

  /// Push an event, waiting if the queue is full (applies backpressure to the producer).
  pub async fn push(&self, event: T) {
    self.channel.send(event).await;
  }

  /// Push an event, dropping it if the queue is full. Safe to call from sync contexts.
  /// Returns `true` if the event was queued.
  pub fn try_push(&self, event: T) -> bool {
    self.channel.try_send(event).is_ok()
  }

  /// Take an event if one is immediately available, otherwise `None`.
  pub fn try_next(&self) -> Option<T> {
    self.channel.try_receive().ok()
  }

  /// Discard any queued events.
  pub fn clear(&self) {
    self.channel.clear();
  }

  pub fn len(&self) -> usize {
    self.channel.len()
  }

  pub fn is_empty(&self) -> bool {
    self.channel.is_empty()
  }
}

impl<T, const N: usize> Default for EventQueue<T, N> {
  fn default() -> Self {
    Self::new()
  }
}

impl<T, const N: usize> Clone for EventQueue<T, N> {
  fn clone(&self) -> Self {
    Self {
      channel: self.channel.clone(),
    }
  }
}

impl<T, const N: usize> fmt::Debug for EventQueue<T, N> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("EventQueue")
      .field("len", &self.len())
      .field("capacity", &N)
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// No-op critical-section lock so embassy-sync primitives link in the host
  /// test binary (the app crate has no `std`-feature lock provider).
  struct TestLock;
  critical_section::set_impl!(TestLock);

  unsafe impl critical_section::Impl for TestLock {
    unsafe fn acquire() -> critical_section::RawRestoreState {
      ()
    }
    unsafe fn release(_restore_state: critical_section::RawRestoreState) {}
  }

  #[test]
  fn test_watched_value_clone() {
    // Ensure it's Clone-able
    let wv = WatchedValue::new(42);
    let _wv2 = wv.clone();
  }

  #[test]
  fn test_try_push_and_try_next() {
    let q = EventQueue::<u32, 2>::new();
    assert!(q.try_push(1));
    assert!(q.try_push(2));
    assert!(!q.try_push(3)); // full
    assert_eq!(q.try_next(), Some(1));
    assert_eq!(q.try_next(), Some(2));
    assert_eq!(q.try_next(), None);
  }

  #[test]
  fn test_clone_shares_storage() {
    let q = EventQueue::<u32, 4>::new();
    let q2 = q.clone();
    q.try_push(42);
    assert_eq!(q2.try_next(), Some(42));
  }
}
