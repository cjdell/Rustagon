#![no_std]

extern crate alloc;

use alloc::format;
use display_types::{Icon20, Icon40, Image, LcdScreen, MenuAnimation, MenuLine};
use embedded_graphics::{
  Drawable as _,
  mono_font::{MonoTextStyle, ascii::FONT_10X20},
  pixelcolor::{Rgb565, Rgb888},
  prelude::{Angle, DrawTarget, Point, RgbColor, Size},
  primitives::{Arc, PrimitiveStyle, Rectangle, RoundedRectangle, StyledDrawable},
  text::{Baseline, Text},
};
use micromath::F32Ext;

// ============================== FrameBuffer trait ==============================

/// A framebuffer that the renderer can draw to. Implementers must also
/// implement `DrawTarget<Color = Rgb565>` for embedded-graphics primitives.
pub trait FrameBuffer: DrawTarget<Color = Rgb565> {
  fn raw_buffer(&mut self) -> &mut [u8];
  fn buffer_width(&self) -> u32;
  fn buffer_height(&self) -> u32;
}

// ============================== Raw pixel drawing ==============================

pub fn draw_raw_image(display: &mut impl FrameBuffer, start_x: i32, start_y: i32, width: u32, height: u32, image: &[u8]) {
  let screen_w = display.buffer_width() as i32;
  let screen_h = display.buffer_height() as i32;

  // Clip the destination rectangle to the visible screen and shift the source
  // window to match. This is required for partially off-screen images — e.g.
  // icons sliding in from the left edge during a menu animation — and avoids
  // the wrap-around arithmetic a negative offset would otherwise produce.
  let clip_x = start_x.max(0);
  let clip_y = start_y.max(0);
  let clip_w = ((start_x + width as i32).min(screen_w) - clip_x).max(0) as usize;
  let clip_h = ((start_y + height as i32).min(screen_h) - clip_y).max(0) as usize;

  if clip_w == 0 || clip_h == 0 {
    return;
  }

  let src_x = (clip_x - start_x) as usize;
  let src_y = (clip_y - start_y) as usize;
  let width = width as usize;
  let sw = screen_w as usize;
  let buf = display.raw_buffer();

  for row in 0..clip_h {
    let src = (src_y + row) * width * 2 + src_x * 2;
    let dst = (clip_y as usize + row) * sw * 2 + clip_x as usize * 2;
    buf[dst..dst + clip_w * 2].copy_from_slice(&image[src..src + clip_w * 2]);
  }
}

// ============================== Icon system ==============================

pub trait Icon {
  fn size(&self) -> Size;
  fn data(&self) -> &[u8];
}

impl Icon for display_types::Image {
  fn size(&self) -> Size {
    Size::new(240, 240)
  }
  fn data(&self) -> &[u8] {
    match &self {
      Image::RustLogo => RUST_LOGO,
    }
  }
}

impl Icon for Icon20 {
  fn size(&self) -> Size {
    Size::new(20, 20)
  }
  fn data(&self) -> &[u8] {
    match &self {
      Icon20::Home => HOME_20,
      Icon20::Config => CONFIG_20,
      Icon20::Wifi => WIFI_20,
      Icon20::File => FILE_20,
      Icon20::Info => INFO_20,
    }
  }
}

impl Icon for Icon40 {
  fn size(&self) -> Size {
    Size::new(40, 40)
  }
  fn data(&self) -> &[u8] {
    match &self {
      Icon40::Info => INFO_40,
      Icon40::Warn => WARN_40,
      Icon40::Error => ERROR_40,
      Icon40::Wifi => WIFI_40,
    }
  }
}

pub fn draw_icon(display: &mut impl FrameBuffer, pos: Point, icon: impl Icon) {
  let size = icon.size();
  let data = icon.data();
  draw_raw_image(display, pos.x, pos.y, size.width, size.height, data);
}

// ============================== Icon data (RGB565 raw) ==============================

use procmacros::include_rgb565_icon;

// On ESP32 (xtensa) icon data is placed in a custom flash section.
// On other targets the default .rodata is used.
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static RUST_LOGO: &[u8] = include_rgb565_icon!("../firmware/assets/images/rust.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static HOME_20: &[u8] = include_rgb565_icon!("../firmware/assets/icons/20x20/home.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static CONFIG_20: &[u8] = include_rgb565_icon!("../firmware/assets/icons/20x20/config.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static WIFI_20: &[u8] = include_rgb565_icon!("../firmware/assets/icons/20x20/wifi.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static FILE_20: &[u8] = include_rgb565_icon!("../firmware/assets/icons/20x20/file.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static INFO_20: &[u8] = include_rgb565_icon!("../firmware/assets/icons/20x20/info.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static INFO_40: &[u8] = include_rgb565_icon!("../firmware/assets/icons/40x40/info.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static WARN_40: &[u8] = include_rgb565_icon!("../firmware/assets/icons/40x40/warn.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static ERROR_40: &[u8] = include_rgb565_icon!("../firmware/assets/icons/40x40/error.png");
#[cfg_attr(target_arch = "xtensa", unsafe(link_section = ".rodata.mydata"))]
static WIFI_40: &[u8] = include_rgb565_icon!("../firmware/assets/icons/40x40/wifi.png");

// ============================== Display constants ==============================

pub const SCREEN_WIDTH: usize = 240;
pub const SCREEN_HEIGHT: usize = 240;

const MARGIN: i32 = 40;
const CHAR_WIDTH: i32 = 10;
const LINE_HEIGHT: i32 = 20;
const USABLE_WIDTH: i32 = SCREEN_WIDTH as i32 - MARGIN * 2;
const USABLE_HEIGHT: i32 = SCREEN_HEIGHT as i32 - MARGIN * 2;
const MAX_LINES: i32 = USABLE_HEIGHT / LINE_HEIGHT;
const OVERFLOW_LINES: i32 = MARGIN / LINE_HEIGHT;
const ICON_WIDTH: i32 = 20;
const ICON_HEIGHT: i32 = 20;

// Notification card constants (GC9A01 is circular — card centred, not a top bar)
const NOTIF_CARD_W: i32 = 190;
const NOTIF_CARD_H: i32 = 70;
const NOTIF_CARD_X: i32 = (SCREEN_WIDTH as i32 - NOTIF_CARD_W) / 2;
const NOTIF_CARD_Y: i32 = 75;
const NOTIF_CARD_RADIUS: i32 = 16;
const NOTIF_SLIDE_DIST: i32 = 100;
const NOTIF_SLIDE_IN_MS: i32 = 350;
const NOTIF_HOLD_MS: i32 = 2_000;
const NOTIF_SLIDE_OUT_MS: i32 = 350;
const NOTIF_TOTAL_MS: i32 = NOTIF_SLIDE_IN_MS + NOTIF_HOLD_MS + NOTIF_SLIDE_OUT_MS;

// ============================== Renderer state ==============================

pub struct LcdState {
  pub screen: LcdScreen,
  underlying: Option<LcdScreen>,
  start_time: i32,
  underlying_start_time: i32,
}

impl LcdState {
  pub fn new(screen: LcdScreen, now_ms: i32) -> Self {
    Self {
      screen,
      underlying: None,
      start_time: now_ms,
      underlying_start_time: now_ms,
    }
  }

  pub fn update(&mut self, new_screen: LcdScreen, now_ms: i32) {
    match &new_screen {
      LcdScreen::Notification(..) => {
        if !matches!(self.screen, LcdScreen::Notification(..)) {
          self.underlying = Some(core::mem::replace(&mut self.screen, new_screen));
          self.underlying_start_time = self.start_time;
          self.start_time = now_ms;
          return;
        }
        self.screen = new_screen;
        self.start_time = now_ms;
      }
      _ => {
        self.underlying = None;
        if should_restart_animation(&self.screen, &new_screen) {
          self.start_time = now_ms;
        }
        self.screen = new_screen;
      }
    }
  }

  pub fn notification_cleanup(&mut self, now_ms: i32) {
    if let LcdScreen::Notification(..) = &self.screen {
      if now_ms - self.start_time >= NOTIF_TOTAL_MS {
        if let Some(underlying) = self.underlying.take() {
          self.start_time = self.underlying_start_time;
          self.screen = underlying;
        }
      }
    }
  }

  pub fn draw(&self, display: &mut impl FrameBuffer, screen: &LcdScreen, now_ms: i32) -> i32 {
    let time_ms = now_ms - self.start_time;

    match screen {
      LcdScreen::Notification(icon, text) => self.draw_notification(display, icon, text, time_ms, now_ms),
      _ => self.draw_screen(display, screen, time_ms, now_ms),
    }
  }
}

// ============================== Screen drawing ==============================

impl LcdState {
  fn draw_screen(&self, display: &mut impl FrameBuffer, screen: &LcdScreen, time_ms: i32, now_ms: i32) -> i32 {
    let style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    match screen {
      LcdScreen::Blank => {}
      LcdScreen::Splash => {
        draw_icon(display, Point::new(0, 0), Image::RustLogo);
      }
      LcdScreen::Headline(icon, headline) => {
        draw_icon(display, Point::new((SCREEN_WIDTH as i32 - 40) / 2, 60), *icon);

        let text_width = headline.chars().count() as i32 * CHAR_WIDTH;
        let mut text = Text::new(
          &headline,
          Point::new((SCREEN_WIDTH as i32 - text_width) / 2, (SCREEN_HEIGHT as i32 - LINE_HEIGHT) / 2),
          style,
        );
        text.text_style.baseline = Baseline::Top;
        text.draw(display).ok();
      }
      LcdScreen::Progress(msg) => {
        let seconds = 5;
        Arc::with_center(
          Point::new(SCREEN_WIDTH as i32 / 2, SCREEN_HEIGHT as i32 / 2),
          200,
          Angle::from_degrees(0.),
          Angle::from_degrees((((360 * time_ms) / (1000 * seconds)) % 360) as f32),
        )
        .draw_styled(&PrimitiveStyle::with_stroke(Rgb565::MAGENTA, 10), display)
        .ok();

        let text_width = msg.chars().count() as i32 * CHAR_WIDTH;
        let mut text = Text::new(
          &msg,
          Point::new((SCREEN_WIDTH as i32 - text_width) / 2, (SCREEN_HEIGHT as i32 - LINE_HEIGHT) / 2),
          style,
        );
        text.text_style.baseline = Baseline::Top;
        text.draw(display).ok();

        return 1;
      }
      LcdScreen::BoundedProgress(transferred, total) => {
        Arc::with_center(
          Point::new(SCREEN_WIDTH as i32 / 2, SCREEN_HEIGHT as i32 / 2),
          200,
          Angle::from_degrees(0.),
          Angle::from_degrees(360. * (*transferred as f32) / (*total as f32)),
        )
        .draw_styled(&PrimitiveStyle::with_stroke(Rgb565::GREEN, 10), display)
        .ok();

        let status = format!("{transferred} of {total}");
        let text_width = status.chars().count() as i32 * CHAR_WIDTH;
        let mut text = Text::new(
          &status,
          Point::new((SCREEN_WIDTH as i32 - text_width) / 2, (SCREEN_HEIGHT as i32 - LINE_HEIGHT) / 2),
          style,
        );
        text.text_style.baseline = Baseline::Top;
        text.draw(display).ok();

        return 1_000;
      }
      LcdScreen::Menu { menu, selected, animation } => {
        return self.draw_menu(display, menu, *selected, *animation, time_ms, now_ms);
      }
      LcdScreen::Notification(..) => {}
    }

    1_000
  }

  fn draw_menu(
    &self,
    display: &mut impl FrameBuffer,
    menu: &[MenuLine],
    selected: u32,
    animation: MenuAnimation,
    time_ms: i32,
    now_ms: i32,
  ) -> i32 {
    let total_items = menu.len() as i32;
    let selected_idx = selected as i32;

    let visible_lines = MAX_LINES;
    let mut start_idx = 0;
    let mut end_idx = total_items.min(visible_lines);

    if total_items > visible_lines {
      let center_line = visible_lines / 2;
      start_idx = (selected_idx - center_line).max(0);
      end_idx = (start_idx + visible_lines).min(total_items);
      if end_idx == total_items {
        start_idx = (total_items - visible_lines).max(0);
      }
    }

    const ANIMATION_DURATION: i32 = 500;
    const START_SCROLLING: i32 = 2000;
    const SCROLLING_PIXELS_PER_SECOND: i32 = 25;

    let slide_dist = SCREEN_WIDTH as i32 - MARGIN;
    let x = match animation {
      MenuAnimation::None => MARGIN,
      MenuAnimation::FromRight => {
        if time_ms < ANIMATION_DURATION {
          SCREEN_WIDTH as i32 - slide_dist * time_ms / ANIMATION_DURATION
        } else {
          MARGIN
        }
      }
      MenuAnimation::FromLeft => {
        if time_ms < ANIMATION_DURATION {
          MARGIN - slide_dist + slide_dist * time_ms / ANIMATION_DURATION
        } else {
          MARGIN
        }
      }
    };

    let render_start_idx = (start_idx - OVERFLOW_LINES).max(0);
    let render_end_idx = (end_idx + OVERFLOW_LINES).min(total_items);

    let mut i = render_start_idx;
    while i < render_end_idx {
      let line = &menu[i as usize];
      let text_width = line.1.len() as i32 * CHAR_WIDTH;
      let y = MARGIN + (i - start_idx) * LINE_HEIGHT;

      let txt_style = if i == selected_idx {
        MonoTextStyle::new(&FONT_10X20, Rgb565::BLACK)
      } else {
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE)
      };

      let mut scroll = 0;
      if i == selected_idx && text_width > USABLE_WIDTH && time_ms >= ANIMATION_DURATION + START_SCROLLING {
        let scroll_speed = 1000 / SCROLLING_PIXELS_PER_SECOND;
        scroll = ((time_ms - ANIMATION_DURATION - START_SCROLLING) / scroll_speed) % (text_width - USABLE_WIDTH);
      }

      let start_x = x - scroll;
      let text_x = start_x + ICON_WIDTH;

      draw_icon(display, Point::new(x, y), line.0);

      if i == selected_idx {
        let s = (((now_ms as f32) / 500.).sin() + 1.) / 4. + 0.5;
        let b = (s * 255.) as u8;
        Rectangle::new(Point::new(text_x, y), Size::new(text_width as u32, ICON_HEIGHT as u32))
          .draw_styled(&PrimitiveStyle::with_fill(Rgb565::from(Rgb888::new(b, b, b))), display)
          .ok();
      }

      let mut text = Text::new(&line.1, Point::new(text_x, y), txt_style);
      text.text_style.baseline = Baseline::Top;
      text.draw(display).ok();

      i += 1;
    }

    // Redraw immediately while a slide is in progress; otherwise keep the 100ms
    // cadence (selected-line blink + long-line scroll). Instant menus skip the
    // busy redraw loop entirely.
    if time_ms < ANIMATION_DURATION && animation != MenuAnimation::None {
      0
    } else {
      100
    }
  }
}

// ============================== Notification drawing ==============================

impl LcdState {
  fn draw_notification(&self, display: &mut impl FrameBuffer, icon: &Icon40, text: &str, elapsed: i32, now_ms: i32) -> i32 {
    if let Some(underlying) = &self.underlying {
      let underlying_elapsed = now_ms - self.underlying_start_time;
      self.draw_screen(display, underlying, underlying_elapsed, now_ms);
    }

    let y_offset = if elapsed < NOTIF_SLIDE_IN_MS {
      let t = elapsed as f32 / NOTIF_SLIDE_IN_MS as f32;
      (-NOTIF_SLIDE_DIST as f32 * (1.0 - smoothstep(t))) as i32
    } else if elapsed < NOTIF_SLIDE_IN_MS + NOTIF_HOLD_MS {
      0
    } else if elapsed < NOTIF_TOTAL_MS {
      let t = (elapsed - NOTIF_SLIDE_IN_MS - NOTIF_HOLD_MS) as f32 / NOTIF_SLIDE_OUT_MS as f32;
      (-NOTIF_SLIDE_DIST as f32 * smoothstep(t)) as i32
    } else {
      -NOTIF_SLIDE_DIST
    };

    let card_top = NOTIF_CARD_Y + y_offset;
    let corner_size = Size::new(NOTIF_CARD_RADIUS as u32, NOTIF_CARD_RADIUS as u32);
    let card_rect = Rectangle::new(
      Point::new(NOTIF_CARD_X, card_top),
      Size::new(NOTIF_CARD_W as u32, NOTIF_CARD_H as u32),
    );

    let icon_x = NOTIF_CARD_X + 16;
    let icon_y = card_top + (NOTIF_CARD_H - 40) / 2;
    let split_x = icon_x + 40 + 8;

    // Full card — dark blue
    RoundedRectangle::with_equal_corners(card_rect, corner_size)
      .draw_styled(&PrimitiveStyle::with_fill(Rgb565::from(Rgb888::new(0, 0, 24))), display)
      .ok();

    // Icon area overlay — black rounded rect clipped to the left portion
    let icon_bg_w = (split_x - NOTIF_CARD_X) as u32;
    let icon_bg = Rectangle::new(Point::new(NOTIF_CARD_X, card_top), Size::new(icon_bg_w, NOTIF_CARD_H as u32));
    RoundedRectangle::with_equal_corners(icon_bg, corner_size)
      .draw_styled(&PrimitiveStyle::with_fill(Rgb565::BLACK), display)
      .ok();

    // Thin border around the whole card
    RoundedRectangle::with_equal_corners(card_rect, corner_size)
      .draw_styled(&PrimitiveStyle::with_stroke(Rgb565::from(Rgb888::new(20, 20, 60)), 1), display)
      .ok();

    draw_icon(display, Point::new(icon_x, icon_y), *icon);

    let text_x = split_x + 8;
    let text_y = card_top + (NOTIF_CARD_H - LINE_HEIGHT) / 2;
    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let mut t = Text::new(text, Point::new(text_x, text_y), text_style);
    t.text_style.baseline = Baseline::Top;
    t.draw(display).ok();

    if elapsed < NOTIF_SLIDE_IN_MS || elapsed >= NOTIF_SLIDE_IN_MS + NOTIF_HOLD_MS {
      0
    } else {
      (NOTIF_SLIDE_IN_MS + NOTIF_HOLD_MS - elapsed).min(200)
    }
  }
}

// ============================== Helpers ==============================

fn should_restart_animation(screen: &LcdScreen, new_screen: &LcdScreen) -> bool {
  match (screen, new_screen) {
    (
      LcdScreen::Menu {
        menu: m1, animation: a1, ..
      },
      LcdScreen::Menu {
        menu: m2, animation: a2, ..
      },
    ) => m1 != m2 || a1 != a2,
    (LcdScreen::Notification(..), LcdScreen::Notification(..)) => true,
    _ => true,
  }
}

fn smoothstep(t: f32) -> f32 {
  t * t * (3.0 - 2.0 * t)
}

fn _u16_to_bytes(raw: u16) -> [u8; 2] {
  [(raw >> 8) as u8, (raw & 0xff) as u8]
}

#[cfg(test)]
mod tests {
  use super::*;
  use alloc::vec;
  use alloc::vec::Vec;
  use embedded_graphics::prelude::RawData as _;

  struct TestFrameBuffer {
    buf: Vec<u8>,
  }

  impl TestFrameBuffer {
    fn new() -> Self {
      Self {
        buf: vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT * 2) as usize],
      }
    }
  }

  impl embedded_graphics::prelude::Dimensions for TestFrameBuffer {
    fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
      embedded_graphics::primitives::Rectangle::new(
        embedded_graphics::prelude::Point::zero(),
        embedded_graphics::prelude::Size::new(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32),
      )
    }
  }

  impl embedded_graphics::prelude::DrawTarget for TestFrameBuffer {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
      I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
      for pixel in pixels {
        let (x, y) = (pixel.0.x, pixel.0.y);
        if x < 0 || x >= SCREEN_WIDTH as i32 || y < 0 || y >= SCREEN_HEIGHT as i32 {
          continue;
        }
        let i = (y as usize * SCREEN_WIDTH + x as usize) * 2;
        let raw: u16 = embedded_graphics::pixelcolor::raw::RawU16::from(pixel.1).into_inner();
        self.buf[i] = (raw >> 8) as u8;
        self.buf[i + 1] = (raw & 0xff) as u8;
      }
      Ok(())
    }
  }

  impl FrameBuffer for TestFrameBuffer {
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

  /// A 2x2 image with a distinct byte pattern per row/column, so placement
  /// (including horizontal/vertical clipping) is easy to assert on.
  fn test_image() -> Vec<u8> {
    (0..8).map(|i| i as u8).collect()
  }

  fn pixel_at(fb: &TestFrameBuffer, x: usize, y: usize) -> [u8; 2] {
    let i = (y * SCREEN_WIDTH + x) * 2;
    [fb.buf[i], fb.buf[i + 1]]
  }

  #[test]
  fn draw_at_negative_x_does_not_panic_and_clips() {
    // Regression test: an icon row at y=0 combined with a negative x offset
    // (FromLeft menu slide) used to wrap the offset arithmetic and panic.
    // A 2x2 image at x=-1 spans -1..1, so only source column 1 is visible.
    let mut fb = TestFrameBuffer::new();
    let image = test_image();

    draw_raw_image(&mut fb, -1, 0, 2, 2, &image);

    assert_eq!(pixel_at(&fb, 0, 0), [2, 3]); // src (1, 0)
    assert_eq!(pixel_at(&fb, 0, 1), [6, 7]); // src (1, 1)
    // The rest of the framebuffer is untouched.
    assert_eq!(pixel_at(&fb, 1, 0), [0, 0]);
  }

  #[test]
  fn draw_fully_off_screen_left_draws_nothing() {
    let mut fb = TestFrameBuffer::new();
    let image = test_image();
    draw_raw_image(&mut fb, -160, 0, 2, 2, &image);
    assert_eq!(pixel_at(&fb, 0, 0), [0, 0]);
  }

  #[test]
  fn draw_clipped_at_right_and_bottom_edges() {
    let mut fb = TestFrameBuffer::new();
    let image = test_image();

    // 2x2 image at (239, 239): only the single pixel (239, 239) is visible.
    draw_raw_image(&mut fb, 239, 239, 2, 2, &image);
    assert_eq!(pixel_at(&fb, 239, 239), [0, 1]);
    assert_eq!(pixel_at(&fb, 238, 239), [0, 0]);
    assert_eq!(pixel_at(&fb, 239, 238), [0, 0]);

    // 2x2 image fully off-screen right.
    let mut fb = TestFrameBuffer::new();
    draw_raw_image(&mut fb, 240, 0, 2, 2, &image);
    assert_eq!(pixel_at(&fb, 239, 0), [0, 0]);
  }

  #[test]
  fn draw_fully_on_screen_matches_layout() {
    let mut fb = TestFrameBuffer::new();
    let image = test_image();
    draw_raw_image(&mut fb, 10, 20, 2, 2, &image);
    assert_eq!(pixel_at(&fb, 10, 20), [0, 1]);
    assert_eq!(pixel_at(&fb, 11, 20), [2, 3]);
    assert_eq!(pixel_at(&fb, 10, 21), [4, 5]);
    assert_eq!(pixel_at(&fb, 11, 21), [6, 7]);
  }
}
