//! App-layer button repeat (Block 10): held hex buttons repeat through
//! `AppRunContext::next`/`next_input` on every platform, `next_raw` never
//! repeats, and the boot button is never a repeat source.
//!
//! Run: `just test` (i.e. `cargo test -p app --features testing`).

extern crate std;

use std::sync::Mutex;

// The only critical-section impl for this test binary (same no-op pattern as
// `app/tests/golden.rs` — every test is serialized by `CLOCK_LOCK`).
struct TestLock;
critical_section::set_impl!(TestLock);
#[allow(clippy::unused_unit)]
unsafe impl critical_section::Impl for TestLock {
  unsafe fn acquire() -> critical_section::RawRestoreState {
    ()
  }
  unsafe fn release(_restore_state: critical_section::RawRestoreState) {}
}

// The embassy-time `MockDriver` is a process-global. Serialize every test
// that advances it so parallel test threads cannot corrupt each other's clocks.
static CLOCK_LOCK: Mutex<()> = Mutex::new(());

use app::apps::{
  AppAction, AppInput, AppRunContext, AppRunEvent, ButtonRepeater, MenuApp, BUTTON_REPEAT_INITIAL_DELAY_MS, BUTTON_REPEAT_PERIOD_MS,
};
use app::testing::{AppDriver, MockPlatform};
use app::types::{HexButton, LcdScreen};
use embassy_time::Duration;
use std::sync::atomic::{AtomicU32, Ordering};

/// Run `f` under the clock lock with a freshly-reset mock clock.
fn with_clock<F: FnOnce()>(f: F) {
  let _guard = CLOCK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
  embassy_time::MockDriver::get().reset();
  f();
}

// ---------------------------------------------------------------------------
// ButtonRepeater unit behaviour
// ---------------------------------------------------------------------------

#[test]
fn repeater_arms_on_press_and_disarms_on_matching_release() {
  with_clock(|| {
    let mut r = ButtonRepeater::new();
    assert!(r.next_repeat().is_none());

    r.feed(HexButton::Down);
    assert_eq!(
      r.next_repeat(),
      Some(embassy_time::Instant::now() + Duration::from_millis(BUTTON_REPEAT_INITIAL_DELAY_MS))
    );

    // A press of the button that is already held (the repeat press flowing
    // back through `feed`) keeps the existing deadline instead of re-arming
    // the initial delay.
    let deadline = r.next_repeat().unwrap();
    r.feed(HexButton::Down);
    assert_eq!(r.next_repeat(), Some(deadline));

    // A release that does not match the held button leaves it armed.
    r.feed(HexButton::LeftReleased);
    assert!(r.next_repeat().is_some());

    // The matching release disarms.
    r.feed(HexButton::DownReleased);
    assert!(r.next_repeat().is_none());

    // A release for nothing held is a no-op.
    r.feed(HexButton::FireReleased);
    assert!(r.next_repeat().is_none());
  });
}

#[test]
fn repeater_rearms_on_new_press_and_elapse_skips_missed_deadlines() {
  with_clock(|| {
    let mut r = ButtonRepeater::new();
    r.feed(HexButton::Down);

    // A newer press re-arms, switching the held button.
    r.feed(HexButton::Left);

    // Miss the deadline by a while: elapse emits once and reschedules at the
    // next period boundary *after now* (no catch-up burst).
    embassy_time::MockDriver::get().advance(Duration::from_millis(1000));
    assert_eq!(r.elapse(), HexButton::Left);

    let now = embassy_time::Instant::now();
    let next = r.next_repeat().unwrap();
    assert!(next > now);
    assert!(next <= now + Duration::from_millis(BUTTON_REPEAT_PERIOD_MS));

    // Elapsed repeats keep the same period.
    embassy_time::MockDriver::get().advance(Duration::from_millis(BUTTON_REPEAT_PERIOD_MS));
    assert_eq!(r.elapse(), HexButton::Left);

    // The deadline is stable across repeated `next_repeat` reads.
    let d1 = r.next_repeat().unwrap();
    let d2 = r.next_repeat().unwrap();
    assert_eq!(d1, d2);
  });
}

// ---------------------------------------------------------------------------
// Repeat through the app run-loop (AppRunContext)
// ---------------------------------------------------------------------------

/// An app that counts `Down` presses into an external counter and quits on
/// `Fire`. `raw` selects `next_raw` (no repeat) vs `next` (repeat).
struct DownCounter {
  count: std::sync::Arc<AtomicU32>,
  raw: bool,
}

impl MenuApp<MockPlatform> for DownCounter {
  fn render(&self) -> LcdScreen {
    LcdScreen::Blank
  }

  async fn run(&mut self, ctx: AppRunContext<'_, MockPlatform>) -> AppAction {
    loop {
      let event = if self.raw { ctx.next_raw().await } else { ctx.next().await };
      match event {
        AppRunEvent::Input(AppInput::Button(HexButton::Down)) => {
          self.count.fetch_add(1, Ordering::Relaxed);
        }
        AppRunEvent::Input(AppInput::Button(HexButton::Fire)) => return AppAction::Stop,
        _ => {}
      }
    }
  }
}

/// Hold Down for ~1 s: the app must see the initial press plus repeats on the
/// 400 ms / 100 ms policy (6 repeats => 7 presses total). Releasing stops the
/// repeats.
#[test]
fn held_button_repeats_in_run_loop() {
  with_clock(|| {
    let count = std::sync::Arc::new(AtomicU32::new(0));
    let mock = MockPlatform::new();
    let mut driver = AppDriver::new(
      DownCounter {
        count: count.clone(),
        raw: false,
      },
      &mock,
    );

    // A raw press (no release) — the button is held.
    mock.push_button_event(HexButton::Down);
    assert!(driver.settle(1000).is_none(), "app should still be running");

    // Release: the repeater disarms, so advancing the clock further must not
    // produce new presses.
    mock.push_button_event(HexButton::DownReleased);
    let held_after_release = count.load(Ordering::Relaxed);
    driver.settle(500);
    assert_eq!(count.load(Ordering::Relaxed), held_after_release, "release must stop the repeats");

    // Quit.
    mock.push_button(HexButton::Fire);
    assert_eq!(driver.settle(100), Some(AppAction::Stop));

    // 1 initial press + repeats every 100 ms after a 400 ms delay over 1000
    // ms => 6 repeats (7 total). Allow small poll-granularity slack.
    let downs = count.load(Ordering::Relaxed);
    assert!((5..=8).contains(&downs), "expected ~7 Down presses, got {downs}");
  });
}

#[test]
fn next_raw_never_repeats() {
  with_clock(|| {
    let count = std::sync::Arc::new(AtomicU32::new(0));
    let mock = MockPlatform::new();
    let mut driver = AppDriver::new(
      DownCounter {
        count: count.clone(),
        raw: true,
      },
      &mock,
    );

    mock.push_button_event(HexButton::Down);
    assert!(driver.settle(1000).is_none(), "app should still be running");

    mock.push_button(HexButton::Fire);
    assert_eq!(driver.settle(100), Some(AppAction::Stop));

    // Exactly the single physical press — no repeats in raw mode.
    assert_eq!(count.load(Ordering::Relaxed), 1, "next_raw must deliver edges 1:1");
  });
}
