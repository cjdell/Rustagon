use crate::utils::{dns::DnsResolver, VecHelper};
use alloc::{
  borrow::ToOwned as _,
  boxed::Box,
  string::{String, ToString as _},
  vec::Vec,
};
use core::future::Future;
use embassy_net::{
  tcp::client::{TcpClient, TcpClientState},
  Stack,
};
use embassy_time::Duration;
use embedded_io_async::Read as _;
use esp_alloc::ExternalMemory;
use log::{debug, error};
use reqwless::{
  client::HttpClient,
  request::{Method, RequestBuilder},
};

use app::protocol::{HttpMethod, HttpRequest, HttpResponseMeta};

/// Map a wire `HttpMethod` onto the concrete reqwless request method.
fn to_reqwless_method(method: HttpMethod) -> Method {
  match method {
    HttpMethod::Get => Method::GET,
    HttpMethod::Post => Method::POST,
    HttpMethod::Put => Method::PUT,
    HttpMethod::Delete => Method::DELETE,
  }
}

/// Perform a streaming HTTP request, invoking `on_meta` once the response
/// headers arrive and `on_chunk` for every chunk of body data as it streams in.
///
/// The request honours `HttpRequest.method` and `HttpRequest.headers`, and sends
/// `HttpRequest.body` as the request payload for non-empty POST/PUT requests.
/// Returns `Ok(())` once the response body is fully drained; `Err(())` if any
/// step of the request fails. The caller is responsible for translating the
/// result into the appropriate `HttpEvent::Done`/`Error` on its channel.
pub async fn perform_http_request_streaming<F1, F2, Fut1, Fut2>(
  stack: Stack<'static>,
  http_request: &HttpRequest,
  mut on_meta: F1,
  mut on_chunk: F2,
) -> Result<(), ()>
where
  F1: FnMut(HttpResponseMeta) -> Fut1,
  F2: FnMut(Vec<u8>) -> Fut2,
  Fut1: Future<Output = ()>,
  Fut2: Future<Output = ()>,
{
  const CHUNK_SIZE: usize = 4096;

  let method = to_reqwless_method(http_request.method);
  debug!("HTTP: request method={} url={}", method.as_str(), http_request.url);

  let state = Box::new_in(TcpClientState::<1, 1024, CHUNK_SIZE>::new(), ExternalMemory);
  let mut tcp_client = TcpClient::new(stack, &state);

  tcp_client.set_timeout(Some(Duration::from_secs(1)));

  let dns = DnsResolver::new(stack);
  let mut client = HttpClient::new(&tcp_client, &dns);

  let mut rx_buf = VecHelper::new_external_buffer(CHUNK_SIZE);

  let headers: Vec<(&str, &str)> = http_request.headers.iter().map(|h| (h.0.as_str(), h.1.as_str())).collect();

  let handle = match client.request(method, &http_request.url).await {
    Ok(handle) => handle,
    Err(err) => {
      error!("HTTP: client.request error: {}", err);
      return Err(());
    }
  };

  // Only POST/PUT carry a body, and only when the caller supplied a non-empty
  // payload; GET/DELETE send none. `Option<&[u8]>` unifies both cases so the
  // resulting handle keeps a single concrete type — the connection it owns must
  // outlive the response, which borrows it for the whole read below. (reqwless
  // implements `RequestBody` for `Option<T>`; `None` sends an empty body.)
  let body_slice: &[u8] = &http_request.body;
  let has_body = matches!(http_request.method, HttpMethod::Post | HttpMethod::Put) && !body_slice.is_empty();
  let body: Option<&[u8]> = if has_body { Some(body_slice) } else { None };

  let mut handle = handle.headers(&headers).body(body);

  let response = match handle.send(&mut rx_buf).await {
    Ok(response) => response,
    Err(err) => {
      error!("HTTP: handle.send error: {}", err);
      return Err(());
    }
  };

  debug!("HTTP: got response status={}", response.status.0);

  let mut meta = HttpResponseMeta::new(response.status.0 as u32);

  for (name, value) in response.headers() {
    if !name.is_empty() {
      meta.headers.push((name.to_owned(), String::from_utf8_lossy(value).to_string()));
    }
  }

  debug!("HTTP: on_meta");
  on_meta(meta).await;

  let mut reader = response.body().reader();

  loop {
    let mut chunk_buf = VecHelper::new_external_buffer(CHUNK_SIZE);
    let mut total_read = 0;

    // Try to fill the buffer completely
    while total_read < CHUNK_SIZE {
      match reader.read(&mut chunk_buf[total_read..]).await {
        Ok(0) => {
          // End of stream
          if total_read > 0 {
            // Send any remaining data
            chunk_buf.truncate(total_read);
            debug!("HTTP: on_chunk len={}", total_read);
            on_chunk(VecHelper::to_global_vec(chunk_buf)).await;
          }
          debug!("HTTP: done");
          return Ok(());
        }
        Ok(n) => {
          total_read += n;

          // If buffer is full, send it and break to get a new buffer
          if total_read == CHUNK_SIZE {
            debug!("HTTP: on_chunk len={}", CHUNK_SIZE);
            on_chunk(VecHelper::to_global_vec(chunk_buf)).await;
            break;
          }
        }
        Err(err) => {
          error!("HTTP: reader.read error: {}", err);
          return Err(());
        }
      }
    }
  }
}
