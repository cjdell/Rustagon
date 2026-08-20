//! A poll-based driver for running a single [`MenuApp`]'s `run()` loop
//! headlessly against a scripted [`MockPlatform`].
//!
//! The app's `run()` future is pinned and driven one poll at a time with a
//! no-op waker. [`AppDriver::settle`] polls until the app is blocked, optionally
//! advancing the embassy-time mock clock (1 ms per poll) so pending timers can
//! fire. Because the app always blocks on its `ctx.next()` multiplexer when
//! idle, settling is bounded: once no internal work remains, further polls just
//! re-block on the (empty) mock input/event queues and the loop stops after a
//! fixed cap.
//!
//! The driver is `no_std` (it only advances the global embassy-time mock
//! driver). Golden-file I/O and test serialization live in the integration
//! tests (which are `std`).

use alloc::boxed::Box;
use core::fmt;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::ptr;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::apps::{AppAction, AppRunContext, MenuApp};
use crate::platform::Platform;

/// The outcome of a single [`AppDriver::poll`].
#[derive(Debug)]
pub enum PollResult {
  Pending,
  Ready(AppAction),
}

/// Hard cap on polls in one [`AppDriver::settle`] call. Idle apps stop early
/// (re-blocking on empty queues); work-driven apps (HTTP, OTA, scan) drain well
/// within this.
const MAX_SETTLE_POLLS: usize = 8_000;

// --- no-op waker (the app is driven synchronously; timers are woken directly
// --- by `MockDriver::advance`) ---

fn noop_clone(_: *const ()) -> RawWaker {
  noop_raw()
}
fn noop_wake(_: *const ()) {}
fn noop_wake_by_ref(_: *const ()) {}
fn noop_drop(_: *const ()) {}

static NOOP_VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop_wake, noop_wake_by_ref, noop_drop);

fn noop_raw() -> RawWaker {
  RawWaker::new(ptr::null(), &NOOP_VTABLE)
}

fn noop_waker() -> Waker {
  unsafe { Waker::from_raw(noop_raw()) }
}

/// Drives one app's `run()` loop. Construct with [`AppDriver::new`]; the app is
/// consumed (owned by the pinned `run()` future) and the platform is borrowed
/// for the driver's lifetime.
pub struct AppDriver<'p, P: Platform> {
  fut: Option<Pin<Box<dyn Future<Output = AppAction> + 'p>>>,
  waker: Waker,
  completed: Option<AppAction>,
  _phantom: PhantomData<&'p P>,
}

impl<'p, P: Platform> AppDriver<'p, P> {
  /// Wrap `app` in its `run()` loop against `platform`. The app is moved into
  /// the pinned future; the platform must outlive the driver.
  pub fn new<F>(app: F, platform: &'p P) -> Self
  where
    F: MenuApp<P> + 'static,
  {
    let fut: Pin<Box<dyn Future<Output = AppAction> + 'p>> = Box::pin(async move {
      let mut app = app;
      app.run(AppRunContext::new(platform, None)).await
    });
    Self {
      fut: Some(fut),
      waker: noop_waker(),
      completed: None,
      _phantom: PhantomData,
    }
  }

  /// Poll the app once. Returns [`PollResult::Ready`] (and records the
  /// [`AppAction`]) if the run loop has returned.
  pub fn poll(&mut self) -> PollResult {
    let fut = self.fut.as_mut().expect("poll after completion");
    let cx = &mut Context::from_waker(&self.waker);
    match fut.as_mut().poll(cx) {
      Poll::Pending => PollResult::Pending,
      Poll::Ready(action) => {
        self.completed = Some(action.clone());
        PollResult::Ready(action)
      }
    }
  }

  /// Poll until the app is blocked. While `i < advance_ms`, advance the mock
  /// clock 1 ms per poll so pending timers fire; afterwards keep polling (up to
  /// [`MAX_SETTLE_POLLS`]) so any internal work drains. Returns the
  /// [`AppAction`] if the run loop completed during settling.
  pub fn settle(&mut self, advance_ms: u64) -> Option<AppAction> {
    let advance = advance_ms as usize;
    for i in 0..MAX_SETTLE_POLLS {
      match self.poll() {
        PollResult::Ready(action) => return Some(action),
        PollResult::Pending => {
          if i < advance {
            embassy_time::MockDriver::get().advance(embassy_time::Duration::from_millis(1));
          }
        }
      }
    }
    None
  }

  /// The run loop's returned [`AppAction`], if it has completed.
  pub fn completed(&self) -> Option<AppAction> {
    self.completed.clone()
  }

  /// True once the run loop has returned.
  pub fn is_done(&self) -> bool {
    self.completed.is_some()
  }
}

impl<P: Platform> fmt::Debug for AppDriver<'_, P> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("AppDriver").field("done", &self.completed.is_some()).finish()
  }
}
