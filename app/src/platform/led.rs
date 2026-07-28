use crate::types::LedRequest;
use alloc::sync::Arc;
use core::fmt;

#[derive(Debug, Clone)]
pub enum LedError {
  ChannelFull,
  ChannelClosed,
}

pub trait LedManager: Send + Sync + fmt::Debug {
  fn request(&self, request: LedRequest) -> Result<(), LedError>;
}

#[derive(Clone, Debug)]
pub struct LedHandle {
  inner: Arc<dyn LedManager>,
}

impl LedHandle {
  pub fn new(manager: Arc<dyn LedManager>) -> Self {
    Self { inner: manager }
  }

  pub fn request(&self, request: LedRequest) -> Result<(), LedError> {
    self.inner.request(request)
  }
}
