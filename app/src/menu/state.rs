use crate::{apps::MenuAppType, menu::RootMenuApp, platform::Platform};
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU8, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

#[derive(Debug, Clone, PartialEq)]
pub enum StackEntryType {
  RootMenu,
  MenuApp,
  HostedApp,
}

pub enum AppStackEntry<P: Platform> {
  RootMenu { menu: RootMenuApp<P> },
  MenuApp { app: MenuAppType<P> },
  HostedApp,
}

impl<P: Platform> AppStackEntry<P> {
  pub fn entry_type(&self) -> StackEntryType {
    match self {
      Self::RootMenu { .. } => StackEntryType::RootMenu,
      Self::MenuApp { .. } => StackEntryType::MenuApp,
      Self::HostedApp => StackEntryType::HostedApp,
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StackEvent {
  Pushed(StackEntryType),
  Popped,
}

const SIGNAL_NONE: u8 = 0;
const SIGNAL_POPPED: u8 = 1;
const SIGNAL_PUSHED: u8 = 2;

pub struct StackSignal {
  state: AtomicU8,
  waker: Signal<CriticalSectionRawMutex, ()>,
}

impl Default for StackSignal {
  fn default() -> Self {
    Self::new()
  }
}

impl StackSignal {
  pub fn new() -> Self {
    Self {
      state: AtomicU8::new(SIGNAL_NONE),
      waker: Signal::new(),
    }
  }

  pub fn send(&self, event: StackEvent) {
    let val = match event {
      StackEvent::Popped => SIGNAL_POPPED,
      StackEvent::Pushed(_) => SIGNAL_PUSHED,
    };
    self.state.store(val, Ordering::Release);
    self.waker.signal(());
  }

  /// Non-blocking check. Returns `Some(event)` if one was sent since the last
  /// `try_receive` or `receive` call.
  pub fn try_receive(&self) -> Option<StackEvent> {
    match self.state.swap(SIGNAL_NONE, Ordering::Acquire) {
      SIGNAL_POPPED => Some(StackEvent::Popped),
      SIGNAL_PUSHED => Some(StackEvent::Pushed(StackEntryType::HostedApp)),
      _ => None,
    }
  }

  /// Block until an event is available, then return it.
  pub async fn receive(&self) -> StackEvent {
    loop {
      if let Some(event) = self.try_receive() {
        return event;
      }
      self.waker.wait().await;
    }
  }

  pub fn reset(&self) {
    self.state.store(SIGNAL_NONE, Ordering::Release);
  }
}

pub type StackEventHandle = Arc<StackSignal>;

pub fn create_stack_event_handle() -> StackEventHandle {
  Arc::new(StackSignal::new())
}
