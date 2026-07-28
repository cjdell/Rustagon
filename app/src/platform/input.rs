use crate::types::HexButton;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::{fmt, future::Future, pin::Pin};

pub trait InputManager: Send + Sync + fmt::Debug {
  fn next_button(&self) -> Pin<Box<dyn Future<Output = HexButton> + Send + '_>>;
  fn inject_button(&self, button: HexButton) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

#[derive(Clone, Debug)]
pub struct InputHandle {
  inner: Arc<dyn InputManager>,
}

impl InputHandle {
  pub fn new(manager: Arc<dyn InputManager>) -> Self {
    Self { inner: manager }
  }

  pub async fn next_button(&self) -> HexButton {
    self.inner.next_button().await
  }

  pub async fn inject_button(&self, button: HexButton) {
    self.inner.inject_button(button).await
  }
}
