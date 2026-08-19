use crate::utils::perform_http_request_streaming;
use alloc::boxed::Box;
use app::platform::{HttpClient, HttpEventChannel};
use app::protocol::{HttpEvent, HttpRequest};
use core::{fmt, future::Future, pin::Pin};
use embassy_net::Stack;

/// Hardware HTTP client wrapping the ESP32 network stack.
///
/// Safety: `Stack<'static>` is `!Send + !Sync` (uses `RefCell` internally),
/// but this type is only used from a single async executor core. All access
/// is serialized by the runtime — same pattern as `SendFilesystem` in AGENTS.md.
pub struct HardwareHttpClient {
  stack: Stack<'static>,
}

impl HardwareHttpClient {
  pub fn new(stack: Stack<'static>) -> Self {
    Self { stack }
  }
}

// Safety: only used from a single async executor core
unsafe impl Send for HardwareHttpClient {}
unsafe impl Sync for HardwareHttpClient {}

impl fmt::Debug for HardwareHttpClient {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareHttpClient").finish()
  }
}

impl Clone for HardwareHttpClient {
  fn clone(&self) -> Self {
    Self { stack: self.stack }
  }
}

impl HttpClient for HardwareHttpClient {
  fn request<'a>(&'a self, req: HttpRequest, channel: &'a HttpEventChannel) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    let stack = self.stack;
    Box::pin(async move {
      let result = perform_http_request_streaming(
        stack,
        &req,
        |meta| async {
          channel.send(HttpEvent::Meta(meta)).await;
        },
        |chunk| async {
          channel.send(HttpEvent::Chunk(chunk)).await;
        },
      )
      .await;

      match result {
        Ok(()) => channel.send(HttpEvent::Done).await,
        Err(()) => channel.send(HttpEvent::Error).await,
      }
    })
  }
}
