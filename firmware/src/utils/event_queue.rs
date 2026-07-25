use alloc::sync::Arc;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

/// A cloneable, heap-allocated event queue for manager -> application event delivery.
///
/// This is the counterpart to [`WatchedValue`](super::watched_value::WatchedValue):
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

#[cfg(test)]
mod tests {
  use super::*;

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
