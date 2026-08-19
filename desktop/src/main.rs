#![feature(impl_trait_in_assoc_type)]
#![recursion_limit = "256"]

mod platform;
mod tasks;

use app::menu::menu_task;
use app::menu::state::{StackEntryType, StackEvent, StackEventHandle};
use app::menu::types::MenuRunnerContext;
use app::platform::Platform;
use app::protocol::{HostIpcChannel, HostIpcMessage, HostIpcSender, HostRuntimeCommand};
use app::types::{
  DeviceEvent, HexButton, HttpReceiver, HttpStatusMessage, KeyCode, KeyEventType, KeyboardEvent, SystemMessage, WebSocketIncomingChannel,
  WebSocketIncomingMessage, WebSocketIncomingReceiver,
};
use display_renderer::{FrameBuffer, LcdState};
use embedded_graphics::prelude::RawData as _;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use platform::DesktopPlatform;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const WIDTH: usize = 240;
const HEIGHT: usize = 240;

fn main() {
  env_logger::builder()
    .filter_level(log::LevelFilter::Info)
    .parse_default_env()
    .init();

  let mut args = std::env::args().skip(1);
  let (data_dir, wasm_app) = match (args.next(), args.next()) {
    (Some(data), Some(app)) => (PathBuf::from(data), resolve_wasm_path(&app)),
    (Some(arg), None) => match resolve_wasm_path(&arg) {
      Some(app) => (default_data_dir(), Some(app)),
      None => (PathBuf::from(arg), None),
    },
    (None, _) => (default_data_dir(), None),
  };
  let wasm_buffer = wasm_app.map(|path| std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display())));
  let platform = Arc::new(DesktopPlatform::new(data_dir));
  // Manager clones sharing the input/system/device-event queues, used by the minifb loop
  let input_pusher = platform.input_pusher.clone();
  let system_pusher = platform.system_pusher.clone();
  let device_event_pusher = platform.device_event_pusher.clone();

  // IPC channels — leaked for static lifetime (never freed on desktop)
  let host_channel = Box::leak(Box::new(HostIpcChannel::new()));
  let host_sender = host_channel.sender();
  let host_receiver = host_channel.receiver();

  // Stack signal — IPC handler sends events, menu runner consumes
  let stack_event_handle = app::menu::state::create_stack_event_handle();
  let stack_event_for_wasm = stack_event_handle.clone();
  let stack_event_for_http = stack_event_handle.clone();

  // App loader: sends StartWasm over IPC when user picks a WASM app.
  let app_loader: Option<app::menu::AppLoader<platform::DesktopPlatform>> =
    Some(|name: String, ctx: app::apps::MenuAppContext<platform::DesktopPlatform>| {
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
    host_ipc_sender: host_sender,
    stack_event_handle,
    app_loader,
    additional_apps: &[],
    auto_launch: wasm_buffer,
  };

  // Spawn the WASM runner thread (analogous to firmware's second_core_task)
  tasks::wasm::spawn_wasm_runner(
    host_receiver,
    host_sender,
    stack_event_for_wasm,
    platform.http_client().unwrap(),
    platform.display_manager(),
    platform.storage_manager(),
  );

  // HTTP server channels — leaked for static lifetime (never freed on desktop)
  let http_channel = Box::leak(Box::new(app::types::HttpChannel::new()));
  let web_socket_incoming_channel = Box::leak(Box::new(WebSocketIncomingChannel::new()));
  let http_sender = http_channel.sender();
  let http_receiver = http_channel.receiver();
  let ws_incoming_sender = web_socket_incoming_channel.sender();
  let ws_incoming_receiver = web_socket_incoming_channel.receiver();

  // Start the HTTP server on a background thread (mirrors firmware's start_http)
  tasks::http::start_http(http_sender, ws_incoming_sender, (*platform).clone());

  // Forward files received over the HTTP API into the WASM runtime (mirrors firmware's ipc_handler)
  let http_forwarder_sender = host_sender;
  std::thread::spawn(move || {
    futures::executor::block_on(http_event_handler(http_receiver, http_forwarder_sender, stack_event_for_http));
  });

  // Forward WebSocket remote-control events into the platform (mirrors firmware's websocket_input_forwarder_task)
  let ws_platform = (*platform).clone();
  std::thread::spawn(move || {
    futures::executor::block_on(websocket_input_forwarder(ws_incoming_receiver, ws_platform));
  });

  // Spawn the menu task on a background thread
  let platform_clone = platform.clone();
  std::thread::spawn(move || {
    futures::executor::block_on(menu_task(runner_ctx));
  });

  // Minifb window on the main thread
  let mut window = Window::new("Rustagon", WIDTH, HEIGHT, WindowOptions::default()).unwrap_or_else(|e| panic!("{}", e));

  // minifb 0.26 only exposes the deprecated name; `set_fps_target` lands in 0.27
  #[allow(deprecated)]
  window.limit_update_rate(Some(std::time::Duration::from_millis(33)));

  let mut fb = vec![0u8; WIDTH * HEIGHT * 2];
  let mut buf32 = vec![0u32; WIDTH * HEIGHT];

  let mut lcd_state = LcdState::new(display_types::LcdScreen::Splash, now_ms());

  while window.is_open() && !window.is_key_down(Key::Escape) {
    // Handle keyboard input: character keys mimic the keyboard hexpansion
    // (typing), while the navigation keys and Ctrl+A-F mimic the badge's hex
    // buttons and boot button.
    push_keyboard_events(&device_event_pusher, &window);
    if let Some(hex) = key_to_hex_button(&window) {
      input_pusher.push_button(hex);
    }
    if let Some(hex) = key_to_hex_button_released(&window) {
      input_pusher.push_button(hex);
    }
    if window.is_key_released(Key::Backspace) {
      system_pusher.push_message(SystemMessage::BootButton);
    }

    let now = now_ms();

    // Check if the WASM has pushed a raw framebuffer
    let wasm_buffer = platform::display::LCD_BUFFER.lock().unwrap().clone();

    let rgb565: &[u8] = if let Some(raw) = &wasm_buffer {
      // Render raw WASM framebuffer (RGB565 LE) directly
      raw.as_slice()
    } else {
      let (screen, _) = platform_clone.get_screen();
      lcd_state.update(screen, now);
      lcd_state.notification_cleanup(now);

      fb.fill(0);
      let mut desk_fb = DesktopFrameBuffer(&mut fb);
      lcd_state.draw(&mut desk_fb, &lcd_state.screen, now);

      &fb
    };

    // Publish the RGB565 frame so frame_buffer()/WebSocket streaming sees the current screen
    platform_clone.display_raw.update_framebuffer(rgb565);

    for y in 0..HEIGHT {
      for x in 0..WIDTH {
        let i = (y * WIDTH + x) * 2;
        let raw = ((rgb565[i] as u16) << 8) | (rgb565[i + 1] as u16);
        let r5 = (raw >> 11) & 0x1F;
        let g6 = (raw >> 5) & 0x3F;
        let b5 = raw & 0x1F;
        let r = (r5 * 255 + 15) / 31;
        let g = (g6 * 255 + 31) / 63;
        let b = (b5 * 255 + 15) / 31;
        buf32[y * WIDTH + x] = (r as u32) << 16 | (g as u32) << 8 | b as u32;
      }
    }

    window.update_with_buffer(&buf32, WIDTH, HEIGHT).unwrap();
  }
}

fn default_data_dir() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("desktop").join("data")
}

/// Resolve a CLI arg to an existing WASM app file, appending `.wsm` if needed.
fn resolve_wasm_path(arg: &str) -> Option<PathBuf> {
  let path = PathBuf::from(arg);
  if path.is_file() {
    return Some(path);
  }
  let with_ext = path.with_extension("wsm");
  if with_ext.is_file() {
    Some(with_ext)
  } else {
    None
  }
}

/// Map a minifb key to the app's `KeyCode`, mimicking the KeebDeck keyboard
/// hexpansion. Arrows/Enter/Backspace are intentionally unmapped — they mimic
/// the badge's hex buttons and boot button instead. Shift is mapped so apps
/// can track shift state (the app crate decides how to shift characters);
/// Ctrl is reserved for hex buttons and the remaining modifiers
/// (Alt/CapsLock) are unmapped — nothing consumes them.
fn key_to_keycode(key: Key) -> Option<KeyCode> {
  use KeyCode::*;
  match key {
    Key::LeftShift | Key::RightShift => Some(Shift),
    Key::A => Some(A),
    Key::B => Some(B),
    Key::C => Some(C),
    Key::D => Some(D),
    Key::E => Some(E),
    Key::F => Some(F),
    Key::G => Some(G),
    Key::H => Some(H),
    Key::I => Some(I),
    Key::J => Some(J),
    Key::K => Some(K),
    Key::L => Some(L),
    Key::M => Some(M),
    Key::N => Some(N),
    Key::O => Some(O),
    Key::P => Some(P),
    Key::Q => Some(Q),
    Key::R => Some(R),
    Key::S => Some(S),
    Key::T => Some(T),
    Key::U => Some(U),
    Key::V => Some(V),
    Key::W => Some(W),
    Key::X => Some(X),
    Key::Y => Some(Y),
    Key::Z => Some(Z),
    Key::Key0 => Some(Digit0),
    Key::Key1 => Some(Digit1),
    Key::Key2 => Some(Digit2),
    Key::Key3 => Some(Digit3),
    Key::Key4 => Some(Digit4),
    Key::Key5 => Some(Digit5),
    Key::Key6 => Some(Digit6),
    Key::Key7 => Some(Digit7),
    Key::Key8 => Some(Digit8),
    Key::Key9 => Some(Digit9),
    Key::Escape => Some(Escape),
    Key::Tab => Some(Tab),
    Key::Space => Some(Space),
    Key::Delete => Some(Delete),
    Key::Home => Some(Home),
    Key::End => Some(End),
    Key::PageUp => Some(PageUp),
    Key::PageDown => Some(PageDown),
    Key::F1 => Some(F1),
    Key::F2 => Some(F2),
    Key::F3 => Some(F3),
    Key::F4 => Some(F4),
    Key::F5 => Some(F5),
    Key::F6 => Some(F6),
    Key::F7 => Some(F7),
    Key::F8 => Some(F8),
    Key::F9 => Some(F9),
    Key::F10 => Some(F10),
    Key::F11 => Some(F11),
    Key::F12 => Some(F12),
    Key::Comma => Some(Comma),
    Key::Period => Some(Period),
    Key::Slash => Some(Slash),
    Key::Semicolon => Some(Semicolon),
    Key::Apostrophe => Some(Quote),
    Key::Minus => Some(Minus),
    Key::Equal => Some(Equals),
    Key::Backquote => Some(Backtick),
    Key::Backslash => Some(Backslash),
    Key::LeftBracket => Some(LBracket),
    Key::RightBracket => Some(RBracket),
    _ => None,
  }
}

fn ctrl_down(window: &Window) -> bool {
  window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl)
}

/// Emit keyboard events for every key pressed/released this frame, mimicking
/// the TCA8418 driver's per-key reporting. Keys pressed while Ctrl is held are
/// reserved for hex buttons and skipped.
fn push_keyboard_events(pusher: &platform::DesktopHexpansionManager, window: &Window) {
  if ctrl_down(window) {
    return;
  }
  for key in window.get_keys_pressed(KeyRepeat::No) {
    if let Some(code) = key_to_keycode(key) {
      pusher.push_device_event(DeviceEvent::Keyboard(KeyboardEvent {
        port: 0,
        typ: KeyEventType::Pressed,
        code,
      }));
    }
  }
  for key in window.get_keys_released() {
    if let Some(code) = key_to_keycode(key) {
      pusher.push_device_event(DeviceEvent::Keyboard(KeyboardEvent {
        port: 0,
        typ: KeyEventType::Released,
        code,
      }));
    }
  }
}

/// Map keys to the badge's hex buttons. Navigation keys (arrows, Enter) work
/// directly; the hex-letter buttons (A-F) require Ctrl so the letters stay
/// available for typing. Emits a press event when the key goes down.
fn key_to_hex_button(window: &Window) -> Option<HexButton> {
  for key in window.get_keys_pressed(KeyRepeat::No) {
    if let Some(hex) = key_to_hex_button_inner(key, window) {
      return Some(hex);
    }
  }
  None
}

/// Emit the release counterpart when a hex-button key goes up.
fn key_to_hex_button_released(window: &Window) -> Option<HexButton> {
  for key in window.get_keys_released() {
    if let Some(hex) = key_to_hex_button_inner(key, window) {
      return Some(hex.released());
    }
  }
  None
}

fn key_to_hex_button_inner(key: Key, window: &Window) -> Option<HexButton> {
  match key {
    Key::Up => Some(HexButton::Up),
    Key::Down => Some(HexButton::Down),
    Key::Right => Some(HexButton::Right),
    Key::Left => Some(HexButton::Left),
    Key::Enter => Some(HexButton::Fire),
    Key::A if ctrl_down(window) => Some(HexButton::HexA),
    Key::B if ctrl_down(window) => Some(HexButton::HexB),
    Key::C if ctrl_down(window) => Some(HexButton::HexC),
    Key::D if ctrl_down(window) => Some(HexButton::HexD),
    Key::E if ctrl_down(window) => Some(HexButton::HexE),
    Key::F if ctrl_down(window) => Some(HexButton::HexF),
    _ => None,
  }
}

fn now_ms() -> i32 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as i32
}

/// Consume files uploaded via the HTTP API (`/api/receive`) and launch them as
/// a WASM program on the stack, mirroring `firmware/src/tasks/ipc_handler.rs`.
async fn http_event_handler(http_receiver: HttpReceiver, host_ipc_sender: HostIpcSender, stack_event_handle: StackEventHandle) {
  loop {
    if let HttpStatusMessage::ReceivedFile(buffer) = http_receiver.receive().await {
      log::info!("http_event_handler: received {} bytes, launching WASM program", buffer.len());
      stack_event_handle.send(StackEvent::Pushed(StackEntryType::HostedApp));
      host_ipc_sender
        .send((0, HostIpcMessage::Runtime(HostRuntimeCommand::StartWasmWithBuffer(buffer))))
        .await;
    }
  }
}

/// Forwards remote control events received over the WebSocket into the platform's
/// input/system event queues, so they are indistinguishable from physical presses.
async fn websocket_input_forwarder(web_socket_incoming_receiver: WebSocketIncomingReceiver, platform: DesktopPlatform) {
  loop {
    match web_socket_incoming_receiver.receive().await {
      WebSocketIncomingMessage::HexButton(hex_button) => {
        platform.input_manager().inject_button(hex_button).await;
      }
      WebSocketIncomingMessage::SystemMessage(message) => {
        platform.system_manager().inject(message).await;
      }
    }
  }
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
