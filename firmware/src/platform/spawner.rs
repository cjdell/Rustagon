//! The firmware's [`AppSpawner`]: tasks run on the app-core (menu) Embassy
//! executor.

use alloc::boxed::Box;
use app::platform::AppSpawner;
use core::fmt;
use core::future::Future;
use core::pin::Pin;
use embassy_executor::{SendSpawner, Spawner};
use log::warn;

/// Spawner bound to the app-core executor. `SendSpawner` is `Send + Sync`, so
/// the handle can live in `HardwarePlatform`; created in `bin/rustagon.rs`
/// from the `#[main]` task's spawner.
#[derive(Clone)]
pub struct FirmwareAppSpawner {
  spawner: SendSpawner,
}

impl fmt::Debug for FirmwareAppSpawner {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("FirmwareAppSpawner").finish_non_exhaustive()
  }
}

impl FirmwareAppSpawner {
  pub fn new(spawner: Spawner) -> Self {
    // `make_send` wraps the same executor; no executor context needed.
    Self {
      spawner: spawner.make_send(),
    }
  }
}

/// Task wrapper for `Send` futures (spawned via `SendSpawner`, whose task
/// state must be `Send`).
#[embassy_executor::task]
async fn run_boxed_send(fut: Box<dyn Future<Output = ()> + Send>) {
  Pin::from(fut).await;
}

/// Task wrapper for `!Send` futures (spawned via the local executor spawner,
/// which has no `Send` bound).
#[embassy_executor::task]
async fn run_boxed_local(fut: Box<dyn Future<Output = ()>>) {
  Pin::from(fut).await;
}

impl AppSpawner for FirmwareAppSpawner {
  fn spawn(&self, fut: Box<dyn Future<Output = ()> + Send + 'static>) {
    match run_boxed_send(fut) {
      Ok(token) => self.spawner.spawn(token),
      Err(err) => warn!("AppSpawner: spawn failed: {err}"),
    }
  }

  fn spawn_local(&self, fut: Box<dyn Future<Output = ()> + 'static>) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    Box::pin(async move {
      // Apps run on the app-core executor (the menu task), so
      // `for_current_executor` here lands on the same executor the app is
      // driving. Same rationale as the TCP pump: all access is serialized by
      // the single-core cooperative runtime, so this is sound.
      let spawner = unsafe { Spawner::for_current_executor() }.await;
      match run_boxed_local(fut) {
        Ok(token) => spawner.spawn(token),
        Err(err) => warn!("AppSpawner: spawn_local failed: {err}"),
      }
    })
  }
}
