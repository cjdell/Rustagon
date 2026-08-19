//! Background-task spawner, abstracted over the platform's executor.
//!
//! Apps normally do *all* their "background" work with `select!` inside their
//! `MenuApp::run()` loop (that is the point of the run-loop model — an SSH
//! handshake and a boot-button interrupt are one `select`, not a task). The
//! spawner exists for genuine long-lived helpers that outlive a single
//! `run()` invocation.
//!
//! # Send-ness
//!
//! - `spawn` accepts `Send` futures. This is the portable path: on desktop it
//!   runs on a background thread, on firmware on the app-core Embassy
//!   executor.
//! - `spawn_local` accepts `!Send` futures. Firmware implements it via
//!   `embassy_executor::Spawner::for_current_executor()`, so the future runs
//!   on the executor the *caller* is on — apps running under the menu must use
//!   `spawn_local` for any future that captures a `!Send` platform type
//!   (embassy-net sockets and connections are the common case). On desktop
//!   all app futures are `Send`, so `spawn_local` simply behaves like `spawn`.

use alloc::boxed::Box;
use alloc::sync::Arc;
use core::fmt;
use core::future::Future;
use core::pin::Pin;

/// A handle to the platform's task spawner. Cloneable and shareable; obtain
/// one from [`Platform::spawner`](crate::platform::Platform::spawner).
pub trait AppSpawner: Send + Sync + fmt::Debug {
  /// Spawn a `Send` background task.
  fn spawn(&self, fut: Box<dyn Future<Output = ()> + Send + 'static>);
  /// Spawn a `!Send` background task on the *caller's* executor. Returns a
  /// future that must be awaited from an async context on that executor (the
  /// firmware resolves the current executor while polling it). See the module
  /// docs for when apps must use this instead of [`spawn`](Self::spawn).
  fn spawn_local(&self, fut: Box<dyn Future<Output = ()> + 'static>) -> Pin<Box<dyn Future<Output = ()> + '_>>;
}

#[derive(Clone, Debug)]
pub struct SpawnerHandle {
  inner: Arc<dyn AppSpawner>,
}

impl SpawnerHandle {
  pub fn new(spawner: Arc<dyn AppSpawner>) -> Self {
    Self { inner: spawner }
  }

  pub fn spawn(&self, fut: Box<dyn Future<Output = ()> + Send + 'static>) {
    self.inner.spawn(fut)
  }

  pub fn spawn_local(&self, fut: Box<dyn Future<Output = ()> + 'static>) -> Pin<Box<dyn Future<Output = ()> + '_>> {
    self.inner.spawn_local(fut)
  }
}
