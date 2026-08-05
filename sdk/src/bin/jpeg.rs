// Decode an inlined JPEG (all in WASM)

#![no_std]
#![no_main]

use crate::lib::{
  fmt::{print_str, print_u32},
  gfx::{SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::print_line,
  protocol::extern_set_lcd_buffer,
  sleep::sleep,
  tasks::spawn,
};
use alloc::boxed::Box;

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    print_line("About to write to LCD...\n");

    let file = include_bytes!("../../assets/leigh.jpg");

    let mut buf = Box::new([0u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);

    let mut decoder = makepad_zune_jpeg::JpegDecoder::new(file);

    let image = log_error!(decoder.decode(), "decode");

    let (image_width, image_height) = decoder.dimensions().unwrap();

    print_str("Image Size: ");
    print_u32(image.len() as u32);
    print_line("\n"); // 37800

    let buf = buf.as_mut_ptr();
    let image = image.as_ptr();

    let mut b1;
    let mut b2 = 0;

    let offset_x = (SCREEN_WIDTH - image_width as usize) / 2;
    let offset_y = (SCREEN_HEIGHT - image_height as usize) / 2;

    for y in 0..image_height as usize {
      b1 = ((y + offset_y) * SCREEN_WIDTH + offset_x) * 2;

      for _x in 0..image_width as usize {
        let r = unsafe { *image.add(b2) } as u16;
        let g = unsafe { *image.add(b2 + 1) } as u16;
        let b = unsafe { *image.add(b2 + 2) } as u16;

        let r5 = (r >> 3) & 0x1F;
        let g6 = (g >> 2) & 0x3F;
        let b5 = (b >> 3) & 0x1F;
        let pixel16 = (r5 << 11) | (g6 << 5) | b5;

        unsafe {
          *buf.add(b1) = ((pixel16 >> 8) & 0xFF) as u8;
          *buf.add(b1 + 1) = (pixel16 & 0xFF) as u8;
        }

        b1 += 2;
        b2 += 3;
      }
    }

    unsafe { extern_set_lcd_buffer(buf) };

    print_line("Done writing to LCD");

    sleep(3000).await;
  })());
}
