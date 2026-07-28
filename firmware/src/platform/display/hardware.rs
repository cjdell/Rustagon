use super::traits::{DisplayError, DisplayManager, LcdSignal};
use super::common::draw_icon;
use crate::{
  d_i2c::*,
  types::*,
  utils::{
    graphics::{BufferTarget, SCREEN_HEIGHT, SCREEN_WIDTH},
    spi::SpiExclusiveDevice,
    *,
  },
};
use alloc::vec::Vec;
use aw9523b::Pin;
use core::fmt;
use core::ptr;
use core::slice::from_raw_parts_mut;
use display_interface::{DataFormat, WriteOnlyDataCommand};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Delay;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::Size;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::{
  Drawable as _,
  mono_font::{MonoTextStyle, ascii::FONT_10X20},
  pixelcolor::Rgb565,
  prelude::{Angle, Point, RgbColor},
  primitives::{Arc, PrimitiveStyle, RoundedRectangle, StyledDrawable},
  text::{Baseline, Text},
};
use esp_alloc::ExternalMemory;
use esp_hal::{
  gpio::{Level, Output, OutputConfig},
  peripherals::Peripherals,
  spi::{
    Mode,
    master::{Config, Spi},
  },
  time::{Instant, Rate},
};
use gc9a01::{
  Gc9a01, SPIDisplayInterface,
  command::Command,
  mode::DisplayConfiguration,
  prelude::{DisplayResolution240x240, DisplayRotation, SPIInterface},
};
use log::info;
use micromath::F32Ext;

impl LcdScreen {
  fn should_restart_animation(screen: &LcdScreen, new_screen: &LcdScreen) -> bool {
    match (screen, new_screen) {
      (LcdScreen::Menu { menu: m1, selected: _ }, LcdScreen::Menu { menu: m2, selected: _ }) => {
        !VecHelper::do_vecs_match(m1, m2)
      }
      (LcdScreen::Notification(..), LcdScreen::Notification(..)) => true,
      _ => true,
    }
  }
}

pub static mut BUFFER: *mut u8 = ptr::null_mut::<u8>();

pub static mut SPI_DISPLAY_INTERFACE: *mut SPIInterface<SpiExclusiveDevice<'_>, Output<'_>> =
  ptr::null_mut::<SPIInterface<SpiExclusiveDevice<'_>, Output<'_>>>();

pub struct HardwareDisplayManager {
  signal: &'static LcdSignal,
}

impl HardwareDisplayManager {
  pub fn new(signal: &'static LcdSignal) -> Self {
    Self { signal }
  }
}

impl fmt::Debug for HardwareDisplayManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareDisplayManager").finish()
  }
}

impl DisplayManager for HardwareDisplayManager {
  fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    self.signal.signal(screen);
    Ok(())
  }

  fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    if self.signal.try_take().is_none() {
      self.signal.signal(screen);
    }
    Ok(())
  }
}

#[embassy_executor::task]
pub async fn lcd_task(sys_bus: MaskedI2cBus, signal: &'static LcdSignal) {
  info!("Starting LCD Task...");

  info!("LCD: Initialising display");

  let p = unsafe { Peripherals::steal() };

  let mut reset = Aw9523bOutputPin::new(sys_bus, I2C_2, Pin::P16);

  let cs = Output::new(p.GPIO1, Level::High, OutputConfig::default());
  let dc = Output::new(p.GPIO2, Level::High, OutputConfig::default());

  let spi = Spi::new(
    p.SPI2,
    Config::default().with_frequency(Rate::from_mhz(80)).with_mode(Mode::_0),
  )
  .unwrap();

  let mut spi = spi.with_sck(p.GPIO8).with_mosi(p.GPIO7);

  let spi_device = SpiExclusiveDevice::new(&mut spi, cs);
  let mut interface = SPIDisplayInterface::new(spi_device, dc);

  let mut buffer = Vec::new_in(ExternalMemory);
  buffer.resize((SCREEN_WIDTH * SCREEN_HEIGHT * 2) as usize, 0u8);

  unsafe {
    BUFFER = buffer.as_mut_ptr();
    SPI_DISPLAY_INTERFACE = core::mem::transmute(&mut interface);
  }

  let mut display = Gc9a01::new(interface, DisplayResolution240x240, DisplayRotation::Rotate0);

  display.reset(&mut reset, &mut Delay).unwrap();
  display.init(&mut Delay).unwrap();
  display.clear().unwrap();

  let raw_buffer = unsafe { from_raw_parts_mut(BUFFER, (SCREEN_WIDTH * SCREEN_HEIGHT * 2) as usize) };
  let interface: &mut DisplayInterface = unsafe { core::mem::transmute(SPI_DISPLAY_INTERFACE) };

  let mut target = BufferTarget::new(buffer);

  let mut state = LcdState::new(LcdScreen::Blank);

  'await_signal: loop {
    state.update(signal.wait().await);

    loop {
      if let Some(new_screen) = signal.try_take() {
        state.update(new_screen);
      }

      // Restore underlying screen if notification animation has finished
      state.notification_cleanup();

      if let LcdScreen::Blank = state.screen {
        continue 'await_signal;
      }

      loop {
        target.clear();

        state.notification_cleanup();

        let next_frame = state.draw(&mut target, &state.screen);

        Command::ColumnAddressSet(0, SCREEN_WIDTH as u16 - 1).send(interface).ok();
        Command::RowAddressSet(0, SCREEN_HEIGHT as u16 - 1).send(interface).ok();
        Command::MemoryWrite.send(interface).ok();

        interface.send_data(DataFormat::U8(raw_buffer)).ok();

        if next_frame == 1_000 {
          continue 'await_signal;
        }
        if next_frame > 0 {
          sleep(next_frame as u64).await;
          break;
        }
      }
    }
  }
}

const MARGIN: i32 = 40;

const CHAR_WIDTH: i32 = 10;
const LINE_HEIGHT: i32 = 20;

const USABLE_WIDTH: i32 = SCREEN_WIDTH as i32 - MARGIN * 2;
const USABLE_HEIGHT: i32 = SCREEN_HEIGHT as i32 - MARGIN * 2;

const MAX_LINES: i32 = USABLE_HEIGHT / LINE_HEIGHT;
const OVERFLOW_LINES: i32 = MARGIN / LINE_HEIGHT;

const ICON_WIDTH: i32 = 20;
const ICON_HEIGHT: i32 = 20;

// Notification card constants (GC9A01 is circular — card is centered, not a top bar)
const NOTIF_CARD_W: i32 = 190;
const NOTIF_CARD_H: i32 = 70;
const NOTIF_CARD_X: i32 = (SCREEN_WIDTH as i32 - NOTIF_CARD_W) / 2;
const NOTIF_CARD_Y: i32 = 75;          // Target top edge of card (fully in the circle)
const NOTIF_CARD_RADIUS: i32 = 16;     // Rounded corners for the card
const NOTIF_SLIDE_DIST: i32 = 100;     // px it travels during slide phases
const NOTIF_SLIDE_IN_MS: i32 = 350;
const NOTIF_HOLD_MS: i32 = 2_000;
const NOTIF_SLIDE_OUT_MS: i32 = 350;
const NOTIF_TOTAL_MS: i32 = NOTIF_SLIDE_IN_MS + NOTIF_HOLD_MS + NOTIF_SLIDE_OUT_MS;

struct LcdState {
  screen: LcdScreen,
  /// Screen that was active before a notification, restored on completion
  underlying: Option<LcdScreen>,
  /// Animation timing baseline for the current (or underlying) screen
  start_time: i32,
  /// Animation timing baseline saved when a notification interrupts — allows
  /// the underlying screen to resume its animation from where it left off
  underlying_start_time: i32,
}

impl LcdState {
  pub fn new(screen: LcdScreen) -> Self {
    let now = Instant::now().duration_since_epoch().as_millis() as i32;
    Self {
      screen,
      underlying: None,
      start_time: now,
      underlying_start_time: now,
    }
  }

  pub fn update(&mut self, new_screen: LcdScreen) -> () {
    let now = Instant::now().duration_since_epoch().as_millis() as i32;
    match &new_screen {
      LcdScreen::Notification(..) => {
        if !matches!(self.screen, LcdScreen::Notification(..)) {
          // First notification — save the current screen + its animation clock
          self.underlying = Some(self.screen.clone());
          self.underlying_start_time = self.start_time;
        }
        self.start_time = now;
        self.screen = new_screen;
      }
      _ => {
        self.underlying = None;
        if LcdScreen::should_restart_animation(&self.screen, &new_screen) {
          self.start_time = now;
        }
        self.screen = new_screen;
      }
    }
  }

  /// Check if a notification has finished and restore the underlying screen.
  /// Called before each draw to ensure we render the right content.
  pub fn notification_cleanup(&mut self) {
    if let LcdScreen::Notification(..) = &self.screen {
      let now = Instant::now().duration_since_epoch().as_millis() as i32;
      if now - self.start_time >= NOTIF_TOTAL_MS {
        if let Some(underlying) = self.underlying.take() {
          self.start_time = self.underlying_start_time;
          self.screen = underlying;
        }
      }
    }
  }

  pub fn draw<'a>(&self, display: &mut BufferTarget, screen: &LcdScreen) -> i32 {
    let now = Instant::now().duration_since_epoch().as_millis() as i32;
    let time_ms = now - self.start_time;

    match screen {
      LcdScreen::Notification(icon, text) => self.draw_notification(display, icon, text, time_ms),
      _ => self.draw_screen(display, screen, time_ms, now),
    }
  }

  fn draw_screen(&self, display: &mut BufferTarget, screen: &LcdScreen, time_ms: i32, now: i32) -> i32 {
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
          Point::new(
            (SCREEN_WIDTH as i32 - text_width) / 2,
            (SCREEN_HEIGHT as i32 - LINE_HEIGHT) / 2,
          ),
          style,
        );
        text.text_style.baseline = Baseline::Top;
        text.draw(display).unwrap();
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
        .unwrap();

        let text_width = msg.chars().count() as i32 * CHAR_WIDTH;

        let mut text = Text::new(
          &msg,
          Point::new(
            (SCREEN_WIDTH as i32 - text_width) / 2,
            (SCREEN_HEIGHT as i32 - LINE_HEIGHT) / 2,
          ),
          style,
        );
        text.text_style.baseline = Baseline::Top;
        text.draw(display).unwrap();

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
        .unwrap();

        let status = alloc::format!("{transferred} of {total}");

        let text_width = status.chars().count() as i32 * CHAR_WIDTH;

        let mut text = Text::new(
          &status,
          Point::new(
            (SCREEN_WIDTH as i32 - text_width) / 2,
            (SCREEN_HEIGHT as i32 - LINE_HEIGHT) / 2,
          ),
          style,
        );
        text.text_style.baseline = Baseline::Top;
        text.draw(display).unwrap();

        return 1_000;
      }
      LcdScreen::Menu { menu, selected } => {
        let total_items = menu.len() as i32;
        let selected_idx = *selected as i32;

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

        let x = if time_ms < ANIMATION_DURATION {
          SCREEN_WIDTH as i32 - ((SCREEN_WIDTH as i32 - MARGIN) * time_ms / ANIMATION_DURATION)
        } else {
          MARGIN
        };

        let render_start_idx = (start_idx - OVERFLOW_LINES).max(0);
        let render_end_idx = (end_idx + OVERFLOW_LINES).min(total_items);

        let mut i = render_start_idx;
        while i < render_end_idx {
          let line = &menu[i as usize];
          let text_width = line.1.len() as i32 * CHAR_WIDTH;
          let y = MARGIN + (i - start_idx) * LINE_HEIGHT;

          let style = if i == selected_idx {
            MonoTextStyle::new(&FONT_10X20, Rgb565::BLACK)
          } else {
            MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE)
          };

          let mut scroll = 0;
          if i == selected_idx && text_width > USABLE_WIDTH && time_ms >= ANIMATION_DURATION + START_SCROLLING {
            let scroll_speed = 1000 / SCROLLING_PIXELS_PER_SECOND;

            let scroll_offset =
              ((time_ms - ANIMATION_DURATION - START_SCROLLING) / scroll_speed) % (text_width - USABLE_WIDTH);
            scroll = scroll_offset;
          }

          let start_x = x - scroll;
          let text_x = start_x + ICON_WIDTH;

          draw_icon(display, Point::new(x, y), line.0);

          if i == selected_idx {
            let s = (((now as f32) / 500.).sin() + 1.) / 4. + 0.5;
            let b = (s * 255.) as u8;
            let col = Rgb565::from(Rgb888::new(b, b, b));

            Rectangle::new(
              Point::new(text_x, y),
              Size {
                width: text_width as u32,
                height: ICON_HEIGHT as u32,
              },
            )
            .draw_styled(&PrimitiveStyle::with_fill(col), display)
            .unwrap();
          }

          let mut text = Text::new(&line.1, Point::new(text_x, y), style);
          text.text_style.baseline = Baseline::Top;
          text.draw(display).unwrap();

          i += 1;
        }

        if x > MARGIN {
          return 0;
        } else {
          return 100;
        }
      }
      _ => {}
    }

    return 1_000;
  }

  /// Draw a notification card that drops into the centre of the circular
  /// GC9A01 display, holds, then slides back out.
  fn draw_notification(&self, display: &mut BufferTarget, icon: &Icon40, text: &str, elapsed: i32) -> i32 {
    // Draw the underlying screen first so the notification overlays it
    if let Some(underlying) = &self.underlying {
      let now = Instant::now().duration_since_epoch().as_millis() as i32;
      let underlying_elapsed = now - self.underlying_start_time;
      self.draw_screen(display, underlying, underlying_elapsed, now);
    }

    // Vertical offset during slide: 0 = target position, negative = above
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

    // Icon (40x40) position
    let icon_x = NOTIF_CARD_X + 16;
    let icon_y = card_top + (NOTIF_CARD_H - 40) / 2;

    // Split point — right edge of icon plus gap
    let split_x = icon_x + 40 + 8;

    // Full card — dark blue (dominant background colour)
    RoundedRectangle::with_equal_corners(card_rect, corner_size)
      .draw_styled(&PrimitiveStyle::with_fill(Rgb565::new(0, 0, 24)), display)
      .unwrap();

    // Icon area overlay — black rounded rect clipped to the left portion.
    // Using RoundedRectangle means its right corners also curve, which keeps
    // the blue background from protruding past the card's corner rounding.
    let icon_bg_w = (split_x - NOTIF_CARD_X) as u32;
    let icon_bg = Rectangle::new(Point::new(NOTIF_CARD_X, card_top), Size::new(icon_bg_w, NOTIF_CARD_H as u32));
    RoundedRectangle::with_equal_corners(icon_bg, corner_size)
      .draw_styled(&PrimitiveStyle::with_fill(Rgb565::BLACK), display)
      .unwrap();

    // Thin border around the whole card
    RoundedRectangle::with_equal_corners(card_rect, corner_size)
      .draw_styled(&PrimitiveStyle::with_stroke(Rgb565::new(20, 20, 60), 1), display)
      .unwrap();

    draw_icon(display, Point::new(icon_x, icon_y), *icon);

    let text_x = split_x + 8;
    let text_y = card_top + (NOTIF_CARD_H - LINE_HEIGHT) / 2;
    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let mut t = Text::new(text, Point::new(text_x, text_y), text_style);
    t.text_style.baseline = Baseline::Top;
    t.draw(display).unwrap();

    // Return redraw interval based on phase
    if elapsed < NOTIF_SLIDE_IN_MS || elapsed >= NOTIF_SLIDE_IN_MS + NOTIF_HOLD_MS {
      0 // Continuous redraw during animation
    } else {
      (NOTIF_SLIDE_IN_MS + NOTIF_HOLD_MS - elapsed).min(200)
    }
  }
}

fn smoothstep(t: f32) -> f32 {
  t * t * (3.0 - 2.0 * t)
}
