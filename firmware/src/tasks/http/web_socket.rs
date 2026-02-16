use crate::{
  lib::{WebSocketIncomingMessage, WebSocketIncomingSender},
  tasks::lcd::BUFFER,
  utils::{
    graphics::{SCREEN_HEIGHT, SCREEN_WIDTH},
    sleep,
  },
};
use alloc::vec;
use alloc::vec::Vec;
use core::slice::from_raw_parts;
use esp_alloc::ExternalMemory;
use esp_println::print;
use log::error;
use picoserve::{
  futures::Either,
  response::ws::{Message, SocketRx, SocketTx, WebSocketCallback},
};

pub struct WebSocketHandler {
  web_socket_incoming_sender: WebSocketIncomingSender,
}

impl WebSocketHandler {
  pub fn new(web_socket_incoming_sender: WebSocketIncomingSender) -> Self {
    Self {
      web_socket_incoming_sender,
    }
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

    let mut message_buffer = Vec::new_in(ExternalMemory);
    message_buffer.resize(4096, 0u8);

    let close_reason = loop {
      let message = match rx.next_message(&mut message_buffer, sleep(250)).await? {
        Either::First(Ok(message)) => message,
        Either::First(Err(error)) => {
          log::warn!("Websocket error: {error:?}");
          break Some((error.code(), "Websocket Error"));
        }
        Either::Second(()) => {
          let raw_buffer = unsafe { from_raw_parts(BUFFER.cast::<u16>(), (SCREEN_WIDTH * SCREEN_HEIGHT) as usize) };

          print!("[");
          match tx.send_binary(&u16_bitmask_to_u8_slice(raw_buffer)).await {
            Ok(()) => {
              print!("]");
              continue;
            }
            Err(err) => {
              error!("Error sending buffer: {err:?}");
              break Some((1011, "Error sending buffer"));
            }
          }
        }
      };

      log::info!("Message: {message:?}");
      match message {
        Message::Text(message) => {
          let message: WebSocketIncomingMessage = serde_json::from_str(message).unwrap();
          self.web_socket_incoming_sender.send(message).await;
        }
        Message::Binary(message) => {
          let message: WebSocketIncomingMessage = serde_json::from_slice(message).unwrap();
          self.web_socket_incoming_sender.send(message).await;
        }
        Message::Close(reason) => {
          log::info!("Websocket close reason: {reason:?}");
          break None;
        }
        Message::Ping(ping) => tx.send_pong(ping).await?,
        Message::Pong(_) => (),
      };
    };

    tx.close(close_reason).await
  }
}
