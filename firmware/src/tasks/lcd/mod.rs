pub mod common;

use crate::lib::{DisplayInterface, Image, LcdScreen};
use crate::tasks::lcd::common::draw_icon;
use crate::utils::VecHelper;
use crate::utils::graphics::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::{graphics::BufferTarget, sleep, spi::SpiExclusiveDevice};
use alloc::{format, vec::Vec};
use core::convert::Infallible;
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
  primitives::{Arc, PrimitiveStyle, StyledDrawable},
  text::{Baseline, Text},
};
use embedded_hal::digital::OutputPin;
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
use gc9a01::command::Logical;
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
      _ => true,
    }
  }
}

pub static mut BUFFER: *mut u8 = ptr::null_mut::<u8>();

pub static mut SPI_DISPLAY_INTERFACE: *mut SPIInterface<SpiExclusiveDevice<'_>, Output<'_>> =
  ptr::null_mut::<SPIInterface<SpiExclusiveDevice<'_>, Output<'_>>>();

pub type LcdSignal = Signal<CriticalSectionRawMutex, LcdScreen>;

struct DummyOutput;

impl embedded_hal::digital::ErrorType for DummyOutput {
  type Error = Infallible;
}

impl OutputPin for DummyOutput {
  fn set_low(&mut self) -> Result<(), Self::Error> {
    Ok(())
  }

  fn set_high(&mut self) -> Result<(), Self::Error> {
    Ok(())
  }
}

#[embassy_executor::task]
pub async fn lcd_task(signal: &'static LcdSignal) {
  info!("Starting LCD Task...");

  info!("LCD: Initialising display");

  let p = unsafe { Peripherals::steal() };

  let mut reset = DummyOutput;
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

  display.reset(&mut reset, &mut Delay);
  display.init(&mut Delay).unwrap();
  display.clear().ok();

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

      if let LcdScreen::Blank = state.screen {
        continue 'await_signal;
      }

      loop {
        target.clear();

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

struct LcdState {
  screen: LcdScreen,
  start_time: i32,
}

impl LcdState {
  pub fn new(screen: LcdScreen) -> Self {
    Self {
      screen,
      start_time: Instant::now().duration_since_epoch().as_millis() as i32,
    }
  }

  pub fn update(&mut self, new_screen: LcdScreen) -> () {
    if LcdScreen::should_restart_animation(&self.screen, &new_screen) {
      self.start_time = Instant::now().duration_since_epoch().as_millis() as i32;
    }

    self.screen = new_screen;
  }

  pub fn draw<'a>(&self, display: &mut BufferTarget, screen: &LcdScreen) -> i32 {
    let now = Instant::now().duration_since_epoch().as_millis() as i32;
    let time_ms = now - self.start_time;

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

        let status = format!("{transferred} of {total}");

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

        // Calculate visible range
        let visible_lines = MAX_LINES;
        let mut start_idx = 0;
        let mut end_idx = total_items.min(visible_lines);

        // Center the selected item if possible
        if total_items > visible_lines {
          let center_line = visible_lines / 2; // middle line index (0-indexed in visible area)
          start_idx = (selected_idx - center_line).max(0);
          end_idx = (start_idx + visible_lines).min(total_items);

          // If at the bottom, snap to bottom
          if end_idx == total_items {
            start_idx = (total_items - visible_lines).max(0);
          }
        }

        const ANIMATION_DURATION: i32 = 500; // Animation: slide menu in from right
        const START_SCROLLING: i32 = 2000; // When to start scrolling long text
        const SCROLLING_PIXELS_PER_SECOND: i32 = 25;

        let x = if time_ms < ANIMATION_DURATION {
          SCREEN_WIDTH as i32 - ((SCREEN_WIDTH as i32 - MARGIN) * time_ms / ANIMATION_DURATION)
        } else {
          MARGIN
        };

        // Allow overflow for the parts of the screen that aren't considered usable
        let render_start_idx = (start_idx - OVERFLOW_LINES).max(0);
        let render_end_idx = (end_idx + OVERFLOW_LINES).min(total_items);

        // Draw each visible item
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

          // Horizontal scrolling for long text on selected item
          let mut scroll = 0;
          if i == selected_idx && text_width > USABLE_WIDTH && time_ms >= ANIMATION_DURATION + START_SCROLLING {
            let scroll_speed = 1000 / SCROLLING_PIXELS_PER_SECOND;

            let scroll_offset =
              ((time_ms - ANIMATION_DURATION - START_SCROLLING) / scroll_speed) % (text_width - USABLE_WIDTH);
            scroll = scroll_offset;
          }

          // Clamp text to usable width
          // let display_width = USABLE_WIDTH.min(text_width);
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

        // Return 0 if animation still running, else 100
        if x > MARGIN {
          return 0;
        } else {
          return 100;
        }
      }
    };

    return 1_000;
  }
}
