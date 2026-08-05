// Demo a HTTP request

#![no_std]
#![no_main]

use sdk as lib;

extern crate alloc;

use crate::lib::{
  gfx::{Canvas, Rgb565},
  http::make_http_request,
  protocol::{HttpRequest, extern_set_lcd_buffer},
  sleep::sleep,
  tasks::spawn,
};
use alloc::{boxed::Box, string::ToString};

#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let mut buf = Box::new([0x00u8; 240 * 240 * 2]);
    let mut canvas = Canvas::new(&mut buf[..], 240, 240);

    let resp = make_http_request(HttpRequest::new("http://firmware.rustagon.chrisdell.info".to_string())).await;

    canvas.draw_text(&resp.body, 0, 0, Rgb565::WHITE, 1);

    unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

    sleep(2_000).await;
  })());
}
