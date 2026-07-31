mod embassy_time_driver;
mod platform;
mod tasks;

use app::menu::menu_task;
use app::menu::types::MenuRunnerContext;
use app::platform::Platform;
use app::protocol::{HostIpcChannel, HostRuntimeCommand};
use app::types::{HexButton, SystemMessage};
use display_renderer::{FrameBuffer, LcdState};
use embedded_graphics::prelude::RawData as _;
use minifb::{Key, Window, WindowOptions};
use platform::{DesktopInputManager, DesktopPlatform, DesktopSystemManager};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const WIDTH: usize = 240;
const HEIGHT: usize = 240;

fn main() {
  env_logger::builder()
    .filter_level(log::LevelFilter::Info)
    .parse_default_env()
    .init();

  let data_dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(default_data_dir);
  let platform = Arc::new(DesktopPlatform::new(data_dir));

  // IPC channels — leaked for static lifetime (never freed on desktop)
  let host_channel = Box::leak(Box::new(HostIpcChannel::new()));
  let host_sender = host_channel.sender();
  let host_receiver = host_channel.receiver();

  // Stack signal — IPC handler sends events, menu runner consumes
  let stack_event_handle = app::menu::state::create_stack_event_handle();
  let stack_event_for_wasm = stack_event_handle.clone();

  // App loader: sends StartWasm over IPC when user picks a WASM app.
  let app_loader: Option<
    fn(String, app::apps::MenuAppContext<platform::DesktopPlatform>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
  > = Some(|name: String, ctx: app::apps::MenuAppContext<platform::DesktopPlatform>| {
    Box::pin(async move {
      log::info!("app_loader: sending StartWasm({name})");
      let result = ctx
        .host_ipc_sender
        .try_send((0, app::protocol::HostIpcMessage::Runtime(HostRuntimeCommand::StartWasm(name))));
      log::info!("app_loader: try_send result={result:?}");
    })
  });

  let runner_ctx = MenuRunnerContext {
    platform: (*platform).clone(),
    host_ipc_sender: host_sender.clone(),
    stack_event_handle,
    app_loader,
    additional_apps: &[],
  };

  // Spawn the WASM runner thread (analogous to firmware's second_core_task)
  tasks::wasm::spawn_wasm_runner(
    host_receiver.clone(),
    host_sender.clone(),
    stack_event_for_wasm,
    platform.http_client().unwrap(),
    platform.display_manager(),
    platform.storage_manager(),
  );

  // Spawn the menu task on a background thread
  let platform_clone = platform.clone();
  std::thread::spawn(move || {
    futures::executor::block_on(menu_task(runner_ctx));
  });

  // Minifb window on the main thread
  let mut window = Window::new("Rustagon", WIDTH, HEIGHT, WindowOptions::default()).unwrap_or_else(|e| panic!("{}", e));

  window.limit_update_rate(Some(std::time::Duration::from_millis(33)));

  let mut fb = vec![0u8; WIDTH * HEIGHT * 2];
  let mut buf32 = vec![0u32; WIDTH * HEIGHT];

  let mut lcd_state = LcdState::new(display_types::LcdScreen::Splash, now_ms());

  while window.is_open() && !window.is_key_down(Key::Escape) {
    // Handle keyboard input
    if let Some(hex) = key_to_hex_button(&window) {
      DesktopInputManager::push_button(hex);
    }
    if window.is_key_released(Key::Backspace) {
      DesktopSystemManager::push_message(SystemMessage::BootButton);
    }

    let now = now_ms();

    // Check if the WASM has pushed a raw framebuffer
    let wasm_buffer = tasks::wasm::LCD_BUFFER.lock().unwrap().clone();

    if let Some(raw) = wasm_buffer {
      // Render raw WASM framebuffer (RGB565 LE) directly
      for y in 0..HEIGHT {
        for x in 0..WIDTH {
          let i = (y * WIDTH + x) * 2;
          let raw565 = ((raw[i] as u16) << 8) | (raw[i + 1] as u16);
          let r5 = (raw565 >> 11) & 0x1F;
          let g6 = (raw565 >> 5) & 0x3F;
          let b5 = raw565 & 0x1F;
          let r = (r5 * 255 + 15) / 31;
          let g = (g6 * 255 + 31) / 63;
          let b = (b5 * 255 + 15) / 31;
          buf32[y * WIDTH + x] = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        }
      }
    } else {
      let (screen, _) = platform_clone.get_screen();
      lcd_state.update(screen, now);
      lcd_state.notification_cleanup(now);

      fb.fill(0);
      let mut desk_fb = DesktopFrameBuffer(&mut fb);
      lcd_state.draw(&mut desk_fb, &lcd_state.screen, now);

      for y in 0..HEIGHT {
        for x in 0..WIDTH {
          let i = (y * WIDTH + x) * 2;
          let raw = ((fb[i] as u16) << 8) | (fb[i + 1] as u16);
          let r5 = (raw >> 11) & 0x1F;
          let g6 = (raw >> 5) & 0x3F;
          let b5 = raw & 0x1F;
          let r = (r5 * 255 + 15) / 31;
          let g = (g6 * 255 + 31) / 63;
          let b = (b5 * 255 + 15) / 31;
          buf32[y * WIDTH + x] = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        }
      }
    }

    window.update_with_buffer(&buf32, WIDTH, HEIGHT).unwrap();
  }
}

fn default_data_dir() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("desktop").join("data")
}

fn key_to_hex_button(window: &Window) -> Option<HexButton> {
  if window.is_key_released(Key::Up) {
    Some(HexButton::Up)
  } else if window.is_key_released(Key::Down) {
    Some(HexButton::Down)
  } else if window.is_key_released(Key::Right) {
    Some(HexButton::Right)
  } else if window.is_key_released(Key::Left) {
    Some(HexButton::Left)
  } else if window.is_key_released(Key::Space) || window.is_key_released(Key::Enter) {
    Some(HexButton::Fire)
  } else if window.is_key_released(Key::A) {
    Some(HexButton::HexA)
  } else if window.is_key_released(Key::B) {
    Some(HexButton::HexB)
  } else if window.is_key_released(Key::C) {
    Some(HexButton::HexC)
  } else if window.is_key_released(Key::D) {
    Some(HexButton::HexD)
  } else if window.is_key_released(Key::E) {
    Some(HexButton::HexE)
  } else if window.is_key_released(Key::F) {
    Some(HexButton::HexF)
  } else {
    None
  }
}

fn now_ms() -> i32 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as i32
}

struct DesktopFrameBuffer<'a>(&'a mut [u8]);

impl embedded_graphics::prelude::Dimensions for DesktopFrameBuffer<'_> {
  fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
    embedded_graphics::primitives::Rectangle::new(
      embedded_graphics::prelude::Point::zero(),
      embedded_graphics::prelude::Size::new(WIDTH as u32, HEIGHT as u32),
    )
  }
}

impl embedded_graphics::prelude::DrawTarget for DesktopFrameBuffer<'_> {
  type Color = embedded_graphics::pixelcolor::Rgb565;
  type Error = core::convert::Infallible;
  fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
  where
    I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
  {
    for pixel in pixels {
      let (x, y) = (pixel.0.x, pixel.0.y);
      if x < 0 || x >= WIDTH as i32 || y < 0 || y >= HEIGHT as i32 {
        continue;
      }
      let i = (y as usize * WIDTH + x as usize) * 2;
      let raw: u16 = embedded_graphics::pixelcolor::raw::RawU16::from(pixel.1).into_inner();
      self.0[i] = (raw >> 8) as u8;
      self.0[i + 1] = (raw & 0xFF) as u8;
    }
    Ok(())
  }
}

impl FrameBuffer for DesktopFrameBuffer<'_> {
  fn raw_buffer(&mut self) -> &mut [u8] {
    self.0
  }
  fn buffer_width(&self) -> u32 {
    WIDTH as u32
  }
  fn buffer_height(&self) -> u32 {
    HEIGHT as u32
  }
}
