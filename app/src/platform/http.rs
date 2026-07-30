use crate::protocol::{HttpEvent, HttpRequest};
use alloc::{boxed::Box, sync::Arc};
use core::{fmt, future::Future, pin::Pin};
use embassy_sync::{
  blocking_mutex::raw::CriticalSectionRawMutex,
  channel::Channel,
};

pub type HttpEventChannel = Channel<CriticalSectionRawMutex, HttpEvent, 2>;

pub trait HttpClient: Send + Sync + fmt::Debug {
  fn request<'a>(
    &'a self,
    req: HttpRequest,
    channel: &'a HttpEventChannel,
  ) -> Pin<Box<dyn Future<Output = ()> + 'a>>;
}

#[derive(Clone, Debug)]
pub struct HttpClientHandle {
  inner: Arc<dyn HttpClient>,
}

impl HttpClientHandle {
  pub fn new(client: Arc<dyn HttpClient>) -> Self {
    Self { inner: client }
  }

  pub async fn request(&self, req: HttpRequest, channel: &HttpEventChannel) {
    self.inner.request(req, channel).await
  }
}
