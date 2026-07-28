use alloc::vec;
use alloc::vec::Vec;
use core::slice::from_raw_parts;
use crate::platform::display::DisplayHandle;
use crate::types::WebSocketIncomingSender;
use crate::types::WebSocketIncomingMessage;
use picoserve::{
  futures::Either,
  response::ws::{Message, SocketRx, SocketTx, WebSocketCallback},
};

pub struct WebSocketHandler {
  web_socket_incoming_sender: WebSocketIncomingSender,
  display: DisplayHandle,
}

impl WebSocketHandler {
  pub fn new(web_socket_incoming_sender: WebSocketIncomingSender, display: DisplayHandle) -> Self {
    Self { web_socket_incoming_sender, display }
  }
}

fn u16_bitmask_to_u8_slice(data: &[u16]) -> Vec<u8> {
  let len_bytes = (data.len() + 7) / 8;
  let mut bitmask = vec![0u8; len_bytes];
  for (i, &value) in data.iter().enumerate() {
    let byte_idx = i / 8;
    let bit_idx = i % 8;
    if value != 0 {
      bitmask[byte_idx] |= 1 << bit_idx;
    }
  }
  bitmask
}

impl WebSocketCallback for WebSocketHandler {
  async fn run<R: picoserve::io::Read, W: picoserve::io::Write<Error = R::Error>>(
    self,
    mut rx: SocketRx<R>,
    mut tx: SocketTx<W>,
  ) -> Result<(), W::Error> {
    use Message;

    let mut message_buffer = Vec::new();
    message_buffer.resize(4096, 0u8);

    let close_reason = loop {
      let message = match rx.next_message(&mut message_buffer, crate::http::sleep(250)).await? {
        Either::First(Ok(message)) => message,
        Either::First(Err(error)) => {
          log::warn!("Websocket error: {error:?}");
          break Some((error.code(), "Websocket Error"));
        }
        Either::Second(()) => {
          let raw_buffer = self.display.frame_buffer().unwrap();
          let pixels = unsafe {
            from_raw_parts(raw_buffer.as_ptr().cast::<u16>(), raw_buffer.len() / 2)
          };

          match tx.send_binary(&u16_bitmask_to_u8_slice(pixels)).await {
            Ok(()) => continue,
            Err(err) => {
              log::error!("Error sending buffer: {err:?}");
              break Some((1011, "Error sending buffer"));
            }
          }
        }
      };

      match message {
        Message::Text(message) => {
          if let Ok(msg) = serde_json::from_str::<WebSocketIncomingMessage>(message) {
            self.web_socket_incoming_sender.send(msg).await;
          }
        }
        Message::Binary(message) => {
          if let Ok(msg) = serde_json::from_slice::<WebSocketIncomingMessage>(message) {
            self.web_socket_incoming_sender.send(msg).await;
          }
        }
        Message::Close(_reason) => {
          break None;
        }
        Message::Ping(ping) => tx.send_pong(ping).await?,
        Message::Pong(_) => (),
      };
    };

    tx.close(close_reason).await
  }
}
