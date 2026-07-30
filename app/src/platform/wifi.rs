use crate::types::{WifiDesiredState, WifiResult};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::{fmt, future::Future, net::Ipv4Addr, pin::Pin};

#[derive(Debug, Clone, PartialEq)]
pub enum WifiStatus {
  Offline,
  Connecting,
  Connected(Ipv4Addr),
  AccessPoint,
  NoNetworksFound,
  Interrupted,
}

pub trait WiFiManager: Send + Sync + fmt::Debug {
  fn get_status(&self) -> Pin<Box<dyn Future<Output = WifiStatus> + Send + '_>>;
  fn wait_for_status_change(&self) -> Pin<Box<dyn Future<Output = WifiStatus> + Send + '_>>;
  fn set_desired_state(&self, state: WifiDesiredState) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
  fn scan(&self) -> Pin<Box<dyn Future<Output = Result<Vec<WifiResult>, ()>> + Send + '_>>;
}

#[derive(Clone, Debug)]
pub struct WiFiHandle {
  inner: Arc<dyn WiFiManager>,
}

impl WiFiHandle {
  pub fn new(manager: Arc<dyn WiFiManager>) -> Self {
    Self { inner: manager }
  }

  pub async fn get_status(&self) -> WifiStatus {
    self.inner.get_status().await
  }

  pub async fn wait_for_status_change(&self) -> WifiStatus {
    self.inner.wait_for_status_change().await
  }

  pub async fn set_desired_state(&self, state: WifiDesiredState) {
    self.inner.set_desired_state(state).await
  }

  pub async fn scan(&self) -> Result<Vec<WifiResult>, ()> {
    self.inner.scan().await
  }
}
