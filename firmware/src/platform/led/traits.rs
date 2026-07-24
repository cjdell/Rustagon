use crate::types::LedRequest;
use alloc::sync::Arc;
use core::fmt;

/// Trait for LED management operations
/// Implementations handle LED control, effects, and animations
pub trait LedManager: Send + Sync + fmt::Debug {
  /// Send an LED request (effect change, color, etc.)
  /// This is non-blocking and should queue the request
  fn request(&self, request: LedRequest) -> Result<(), LedError>;
}

#[derive(Debug, Clone)]
pub enum LedError {
  ChannelFull,
  ChannelClosed,
}

/// A cloneable reference to an LED manager
#[derive(Clone, Debug)]
pub struct LedHandle {
  manager: Arc<dyn LedManager>,
}

impl LedHandle {
  pub fn new(manager: Arc<dyn LedManager>) -> Self {
    Self { manager }
  }

  pub fn request(&self, request: LedRequest) -> Result<(), LedError> {
    self.manager.request(request)
  }
}
