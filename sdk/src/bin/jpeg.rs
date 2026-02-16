// Decode an inlined JPEG (all in WASM)

#![no_std]
#![no_main]
#![feature(future_join)]
#![feature(thread_local)]

use crate::lib::{
  graphics::{SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::set_lcd_buffer,
  sleep::sleep,
  tasks::spawn,
};
use alloc::boxed::Box;

#[path = "../lib/mod.rs"]
#[macro_use]
mod lib;

extern crate alloc;

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    println!("About to write to LCD...");

    let file = include_bytes!("../../assets/leigh.jpg");

    let mut buf = Box::new([0u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);

    let mut decoder = makepad_zune_jpeg::JpegDecoder::new(file);

    let image = log_error!(decoder.decode(), "decode");

    let (image_width, image_height) = decoder.dimensions().unwrap();

    println!("Image Size: {}", image.len()); // 37800

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

    set_lcd_buffer(buf);

    println!("Done writing to LCD");

    sleep(3000).await;
  })());
}
