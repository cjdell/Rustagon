use alloc::vec::Vec;
use display_renderer::FrameBuffer;
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
}

impl FrameBuffer for BufferTarget {
  fn raw_buffer(&mut self) -> &mut [u8] {
    &mut self.buf
  }

  fn buffer_width(&self) -> u32 {
    SCREEN_WIDTH as u32
  }

  fn buffer_height(&self) -> u32 {
    SCREEN_HEIGHT as u32
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
