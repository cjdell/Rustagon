//! Block 10: prove the menu/headline icons actually render on the desktop
//! host (the historical "icons empty on non-xtensa" report). The icon data is
//! embedded at compile time by `include_rgb565_icon!` (same macro path as the
//! firmware — no divergent rendering), so this test exercises exactly the
//! renderer path the minifb window uses.

use display_renderer::{draw_icon, FrameBuffer, LcdState};
use display_types::{Icon20, Icon40, Image, LcdScreen, MenuAnimation, MenuLine};
use embedded_graphics::{
  prelude::{Dimensions, DrawTarget, Point, RawData as _},
  primitives::Rectangle,
};

const W: i32 = 240;
const H: i32 = 240;

struct Fb(Vec<u8>);

impl Fb {
  fn new() -> Self {
    Self(vec![0u8; (W * H * 2) as usize])
  }

  /// Count lit (non-black) pixels in a region.
  fn lit(&self, x0: i32, y0: i32, w: i32, h: i32) -> usize {
    let mut n = 0;
    for y in y0..y0 + h {
      for x in x0..x0 + w {
        let i = ((y * W + x) as usize) * 2;
        if self.0[i] != 0 || self.0[i + 1] != 0 {
          n += 1;
        }
      }
    }
    n
  }
}

impl Dimensions for Fb {
  fn bounding_box(&self) -> Rectangle {
    Rectangle::new(Point::zero(), embedded_graphics::prelude::Size::new(W as u32, H as u32))
  }
}

impl DrawTarget for Fb {
  type Color = embedded_graphics::pixelcolor::Rgb565;
  type Error = core::convert::Infallible;

  fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
  where
    I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
  {
    for pixel in pixels {
      let (x, y) = (pixel.0.x, pixel.0.y);
      if x < 0 || x >= W || y < 0 || y >= H {
        continue;
      }
      let i = ((y * W + x) as usize) * 2;
      let raw: u16 = embedded_graphics::pixelcolor::raw::RawU16::from(pixel.1).into_inner();
      self.0[i] = (raw >> 8) as u8;
      self.0[i + 1] = (raw & 0xFF) as u8;
    }
    Ok(())
  }
}

impl FrameBuffer for Fb {
  fn raw_buffer(&mut self) -> &mut [u8] {
    &mut self.0
  }
  fn buffer_width(&self) -> u32 {
    W as u32
  }
  fn buffer_height(&self) -> u32 {
    H as u32
  }
}

#[test]
fn all_icon_assets_are_not_empty() {
  // Every embedded icon must contain lit pixels — a blank asset would mean
  // the proc macro embedded nothing (the old non-xtensa failure mode).
  let icons20: [Icon20; 5] = [Icon20::Home, Icon20::Config, Icon20::Wifi, Icon20::File, Icon20::Info];
  for icon in icons20 {
    let mut fb = Fb::new();
    draw_icon(&mut fb, Point::new(0, 0), icon);
    assert!(fb.lit(0, 0, 20, 20) > 10, "Icon20 {icon:?} rendered blank");
  }
  let icons40: [Icon40; 4] = [Icon40::Info, Icon40::Warn, Icon40::Error, Icon40::Wifi];
  for icon in icons40 {
    let mut fb = Fb::new();
    draw_icon(&mut fb, Point::new(0, 0), icon);
    assert!(fb.lit(0, 0, 40, 40) > 40, "Icon40 {icon:?} rendered blank");
  }
  let mut fb = Fb::new();
  draw_icon(&mut fb, Point::new(0, 0), Image::RustLogo);
  assert!(fb.lit(0, 0, 240, 240) > 100, "RustLogo rendered blank");
}

#[test]
fn menu_lines_and_headlines_draw_icons_through_lcd_state() {
  // Menu: first line's icon sits at (40, 40), 20x20 (MARGIN=40).
  let screen = LcdScreen::Menu {
    menu: vec![
      MenuLine(Icon20::Wifi, "Rustagon Lab XXXX".to_string()),
      MenuLine(Icon20::File, "hello.wsm".to_string()),
    ],
    selected: 0,
    animation: MenuAnimation::None,
  };
  let lcd = LcdState::new(screen, 1000);
  let mut fb = Fb::new();
  lcd.draw(&mut fb, &lcd.screen, 1000);
  assert!(fb.lit(40, 40, 20, 20) > 10, "menu line 1 icon missing");
  assert!(fb.lit(40, 60, 20, 20) > 10, "menu line 2 icon missing");

  // Headline: 40x40 icon centred at x=100, y=60.
  let screen = LcdScreen::Headline(Icon40::Warn, "WiFi disabled".to_string());
  let lcd = LcdState::new(screen, 1000);
  let mut fb = Fb::new();
  lcd.draw(&mut fb, &lcd.screen, 1000);
  assert!(fb.lit(100, 60, 40, 40) > 40, "headline icon missing");
}
