use crate::utils::{VecHelper, dns::DnsResolver};
use alloc::{
  borrow::ToOwned as _,
  boxed::Box,
  string::{String, ToString as _},
  vec::Vec,
};
use core::future::join;
use embassy_net::{
  Stack,
  tcp::client::{TcpClient, TcpClientState},
};
use embassy_sync::{
  blocking_mutex::raw::NoopRawMutex,
  channel::{Channel, Sender},
};
use embassy_time::Duration;
use embedded_io_async::Read as _;
use esp_alloc::ExternalMemory;
use log::error;
use reqwless::{
  client::HttpClient,
  request::{Method, RequestBuilder},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpRequest {
  pub url: String,
  pub headers: Vec<(String, String)>,
}

impl HttpRequest {
  pub fn new(url: String) -> Self {
    Self {
      url,
      headers: Vec::new(),
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpResponseMeta {
  pub status: u32,
  pub headers: Vec<(String, String)>,
}

impl HttpResponseMeta {
  pub fn new(status: u32) -> Self {
    Self {
      status,
      headers: Vec::new(),
    }
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpResponse {
  pub meta: HttpResponseMeta,
  pub body: Vec<u8>,
}

impl HttpResponse {
  pub fn new(meta: HttpResponseMeta, body: Vec<u8>) -> Self {
    Self { meta, body }
  }
}

pub enum HttpEvent {
  Meta(HttpResponseMeta),
  Chunk(Vec<u8>),
  Done,
}

pub async fn perform_http_request(stack: Stack<'static>, req: HttpRequest) -> Result<HttpResponse, ()> {
  let mut meta: Option<HttpResponseMeta> = None;
  let mut body = Vec::new_in(ExternalMemory);

  let channel = Channel::<NoopRawMutex, HttpEvent, 1>::new();

  let request = perform_http_request_channel(stack, channel.sender(), &req);

  let listen = async {
    loop {
      match channel.receive().await {
        HttpEvent::Meta(http_response_meta) => {
          // println!("perform_http_request: Got meta");
          meta = Some(http_response_meta);
        }
        HttpEvent::Chunk(chunk) => {
          // println!("perform_http_request: Got chunk");
          body.extend_from_slice(&chunk);
        }
        HttpEvent::Done => {
          // println!("perform_http_request: Done");
          return;
        }
      }
    }
  };

  let (result, _) = join!(request, listen).await;
  if let Err(err) = result {
    return Err(err);
  };

  Ok(HttpResponse::new(meta.unwrap(), VecHelper::to_global_vec(body)))
}

pub async fn perform_http_request_channel<'a>(
  stack: Stack<'static>,
  sender: Sender<'a, NoopRawMutex, HttpEvent, 1>,
  http_request: &HttpRequest,
) -> Result<(), ()> {
  let result = perform_http_request_streaming(
    stack,
    &http_request,
    |meta| sender.send(HttpEvent::Meta(meta)),
    |chunk| sender.send(HttpEvent::Chunk(chunk)),
  )
  .await;

  sender.send(HttpEvent::Done).await;

  result
}

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

  let state = Box::new_in(TcpClientState::<1, 1024, CHUNK_SIZE>::new(), ExternalMemory);
  let mut tcp_client = TcpClient::new(stack, &state);

  tcp_client.set_timeout(Some(Duration::from_secs(1)));

  let dns = DnsResolver::new(stack);
  let mut client = HttpClient::new(&tcp_client, &dns);

  let mut rx_buf = VecHelper::new_external_buffer(CHUNK_SIZE);

  let headers: Vec<(&str, &str)> = http_request.headers.iter().map(|h| (h.0.as_str(), h.1.as_str())).collect();

  let handle = match client.request(Method::GET, &http_request.url).await {
    Ok(handle) => handle,
    Err(err) => {
      error!("client.request: {}", err);
      return Err(());
    }
  };

  let mut handle = handle.headers(&headers);

  let response = match handle.send(&mut rx_buf).await {
    Ok(response) => response,
    Err(err) => {
      error!("handle.send: {}", err);
      return Err(());
    }
  };

  let mut meta = HttpResponseMeta::new(response.status.0 as u32);

  for (name, value) in response.headers() {
    if !name.is_empty() {
      meta.headers.push((name.to_owned(), String::from_utf8_lossy(value).to_string()));
    }
  }

  // println!("perform_http_request_streaming: Meta: {:?}", meta);
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
            // println!("perform_http_request_streaming: Final chunk: {}", total_read);
            on_chunk(VecHelper::to_global_vec(chunk_buf)).await;
          }
          // println!("perform_http_request_streaming: Finished");
          return Ok(());
        }
        Ok(n) => {
          total_read += n;

          // If buffer is full, send it and break to get a new buffer
          if total_read == CHUNK_SIZE {
            // println!("perform_http_request_streaming: Full chunk: {}", CHUNK_SIZE);
            on_chunk(VecHelper::to_global_vec(chunk_buf)).await;
            break;
          }
        }
        Err(err) => {
          error!("reader.read: {}", err);
          return Err(());
        }
      }
    }
  }
}
