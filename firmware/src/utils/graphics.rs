use alloc::vec::Vec;
use embedded_graphics::{
  Pixel,
  pixelcolor::{Rgb565, raw::RawU16},
  prelude::{Dimensions, DrawTarget, Point, RawData as _, Size},
  primitives::Rectangle,
};
use esp_alloc::ExternalMemory;

pub const SCREEN_WIDTH: usize = 240;
pub const SCREEN_HEIGHT: usize = 240;

pub struct BufferTarget {
  pub buf: Vec<u8, ExternalMemory>,
}

impl BufferTarget {
  pub fn new(buf: Vec<u8, ExternalMemory>) -> Self {
    Self { buf }
  }

  pub fn clear(&mut self) {
    self.buf.fill(0u8);
  }

  pub fn get_buffer_ptr(&self) -> *const u8 {
    self.buf.as_ptr()
  }

  pub fn draw_raw_image(&mut self, start_x: i32, start_y: i32, width: u32, height: u32, image: &[u8]) {
    // println!("draw_raw_image: {start_x} {start_y} {width} {height}");

    let start_x = start_x as usize;
    let start_y = start_y as usize;
    let width = width as usize;
    let height = height as usize;
    let screen_width = SCREEN_WIDTH as usize;

    // Early bounds check
    if start_y >= SCREEN_HEIGHT {
      return;
    }

    let max_y = height.min(SCREEN_HEIGHT.saturating_sub(start_y));

    for icon_y in 0..max_y {
      let source_row_offset = icon_y * width * 2;
      let target_row_offset = (icon_y + start_y) * screen_width * 2 + start_x * 2;

      // Copy entire row at once using copy_from_slice
      let source_start = source_row_offset;
      let source_end = source_start + width * 2;
      let target_start = target_row_offset;
      let target_end = target_start + width * 2;

      if target_end <= self.buf.len() {
        // println!("{target_start}..{target_end} {source_start}..{source_end}");
        self.buf[target_start..target_end].copy_from_slice(&image[source_start..source_end]);
      }
    }
  }
}

impl Dimensions for BufferTarget {
  fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
    Rectangle::new(Point::zero(), Size::new(240, 240))
  }
}

impl DrawTarget for BufferTarget {
  type Color = Rgb565;
  type Error = core::convert::Infallible;

  fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
  where
    I: IntoIterator<Item = Pixel<Self::Color>>,
  {
    // Assume all pixels are within bounds (caller guarantees this)
    // This removes ALL branching and bounds checking — maximum performance
    for Pixel(pos, color) in pixels {
      let i = ((pos.y as usize) * 240 + (pos.x as usize)) * 2;

      if pos.x < 0 || pos.x >= 240 {
        continue;
      }
      if pos.y < 0 || pos.y >= 240 {
        continue;
      }

      let color: RawU16 = color.into();
      let raw: u16 = color.into_inner();

      self.buf[i] = (raw >> 8) as u8;
      self.buf[i + 1] = (raw & 0x0ff) as u8;
    }

    Ok(())
  }
}
