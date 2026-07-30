use crate::types::{HexpansionEvent, HexpansionInfo};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::{fmt, future::Future, pin::Pin};

pub trait HexpansionManager: Send + Sync + fmt::Debug {
  fn next_event(&self) -> Pin<Box<dyn Future<Output = HexpansionEvent> + Send + '_>>;
  fn try_next_event(&self) -> Option<HexpansionEvent>;
  fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)>;
}

#[derive(Clone, Debug)]
pub struct HexpansionHandle {
  inner: Arc<dyn HexpansionManager>,
}

impl HexpansionHandle {
  pub fn new(manager: Arc<dyn HexpansionManager>) -> Self {
    Self { inner: manager }
  }

  pub async fn next_event(&self) -> HexpansionEvent {
    self.inner.next_event().await
  }

  pub fn try_next_event(&self) -> Option<HexpansionEvent> {
    self.inner.try_next_event()
  }

  pub fn current_state(&self) -> Vec<(u8, Option<HexpansionInfo>)> {
    self.inner.current_state()
  }
}
