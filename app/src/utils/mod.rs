//! Clock & timeout helpers shared by apps.
//!
//! `app` is `no_std` and runs on whatever executor the platform provides
//! (Embassy on firmware, std futures on desktop), so time comes from
//! `embassy-time` — the one monotonic clock both hosts agree on.

pub mod sync;

pub use sync::{EventQueue, WatchedValue};

use core::future::Future;
use embassy_futures::select::{select, Either};

pub async fn sleep(ms: u64) {
  embassy_time::Timer::after(embassy_time::Duration::from_millis(ms)).await;
}

/// Monotonic clock in milliseconds. Useful for idle timers, "time since X"
/// tracking, and measuring timeouts in apps.
pub fn now() -> u64 {
  embassy_time::Instant::now().as_millis()
}

/// Run `fut` but give up after `ms` milliseconds. Returns `Some(result)` if
/// `fut` completes in time, `None` on timeout.
pub async fn select_timeout<T, F>(fut: F, ms: u64) -> Option<T>
where
  F: Future<Output = T>,
{
  match select(fut, sleep(ms)).await {
    Either::First(v) => Some(v),
    Either::Second(_) => None,
  }
}

/// Run `fut` but cap it at `ms` milliseconds. On timeout, run `on_timeout` (an
/// async block, typically doing error handling) and return `Err(())`; on
/// completion return `Ok(result)`.
pub async fn with_timeout<T, F, O>(fut: F, ms: u64, on_timeout: O) -> Result<T, ()>
where
  F: Future<Output = T>,
  O: Future<Output = ()>,
{
  match select(fut, sleep(ms)).await {
    Either::First(v) => Ok(v),
    Either::Second(_) => {
      on_timeout.await;
      Err(())
    }
  }
}

/// Try `op` up to `attempts` times, sleeping `delay_ms` between attempts.
/// Returns the first `Ok`, or the last `Err` if all attempts fail. Handy for
/// connect / reconnect loops:
///
/// ```ignore
/// let conn = retry(3, 500, || app::net::connect(host, port)).await?;
/// ```
pub async fn retry<T, E, F, O>(attempts: u32, delay_ms: u64, mut op: F) -> Result<T, E>
where
  F: FnMut() -> O,
  O: Future<Output = Result<T, E>>,
{
  debug_assert!(attempts > 0, "retry needs at least one attempt");
  let mut last_err = None;
  for attempt in 0..attempts {
    match op().await {
      Ok(v) => return Ok(v),
      Err(e) => {
        last_err = Some(e);
        if attempt + 1 < attempts {
          sleep(delay_ms).await;
        }
      }
    }
  }
  Err(last_err.expect("attempts is > 0"))
}
