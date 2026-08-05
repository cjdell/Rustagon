// Showcase for the zero-dependency `gfx` library (sdk/src/lib/gfx).
//
// Cycles through animated scenes exercising every primitive on a 240x240
// RGB565 canvas: palette/pixels, lines, rects, circles, triangles, text,
// and a combined "everything" screen. No embedded_graphics is used here.
//
// Controls:
//   Up / Down - previous / next scene
//   Fire      - pause / resume
// Scenes also auto-advance every 8 seconds.

#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt,
  gfx::{Canvas, Point, Rect, Rgb565, text_height, text_width},
  helper::get_millis,
  protocol::{HexButton, HostIpcMessage, extern_set_lcd_buffer},
  tasks::{HOST_IPC_CHANNEL, spawn, yield_now},
};
use alloc::boxed::Box;

const SCREEN_W: usize = 240;
const SCREEN_H: usize = 240;
const SCENE_MS: u32 = 8000;

// ---------------------------------------------------------------------------
// Scenes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Scene {
  Palette,
  Lines,
  Rects,
  Circles,
  Triangles,
  Text,
  All,
}

const SCENES: [Scene; 7] = [
  Scene::Palette,
  Scene::Lines,
  Scene::Rects,
  Scene::Circles,
  Scene::Triangles,
  Scene::Text,
  Scene::All,
];

fn scene_name(scene: Scene) -> &'static str {
  match scene {
    Scene::Palette => "Palette",
    Scene::Lines => "Lines",
    Scene::Rects => "Rects",
    Scene::Circles => "Circles",
    Scene::Triangles => "Triangles",
    Scene::Text => "Text",
    Scene::All => "All",
  }
}

// ---------------------------------------------------------------------------
// Tiny deterministic PRNG for scene randomness
// ---------------------------------------------------------------------------

struct XorShift64(u64);

impl XorShift64 {
  fn new(seed: u64) -> Self {
    XorShift64(if seed == 0 { 1 } else { seed })
  }

  fn next_u32(&mut self) -> u32 {
    let mut x = self.0;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    self.0 = x;
    x as u32
  }

  fn color(&mut self) -> Rgb565 {
    Rgb565::from_rgb888(
      (self.next_u32() & 0xFF) as u8,
      (self.next_u32() & 0xFF) as u8,
      (self.next_u32() & 0xFF) as u8,
    )
  }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let mut buf = Box::new([0u8; SCREEN_W * SCREEN_H * 2]);
    let mut canvas = Canvas::new(&mut buf[..], SCREEN_W, SCREEN_H);

    let mut subscriber = log_error!(HOST_IPC_CHANNEL.subscriber(), "subscriber");

    let mut scene_idx = 0usize;
    let mut paused = false;
    let mut frame = 0u32;
    let mut scene_start = get_millis();
    let mut fps = 0u32;
    let mut fps_start = get_millis();
    let mut fps_frames = 0u32;

    loop {
      // Button input
      if let Some((_, msg)) = subscriber.try_next_message_pure() {
        if let HostIpcMessage::HexButton(btn) = msg {
          match btn {
            HexButton::Up => {
              scene_idx = (scene_idx + SCENES.len() - 1) % SCENES.len();
              scene_start = get_millis();
            }
            HexButton::Down => {
              scene_idx = (scene_idx + 1) % SCENES.len();
              scene_start = get_millis();
            }
            HexButton::Fire => paused = !paused,
            _ => {}
          }
        }
      }

      // Auto-advance scenes
      let now = get_millis();
      if now.wrapping_sub(scene_start) >= SCENE_MS {
        scene_idx = (scene_idx + 1) % SCENES.len();
        scene_start = now;
      }

      if !paused {
        draw_scene(&mut canvas, SCENES[scene_idx], frame);
        frame = frame.wrapping_add(1);
      }

      // FPS over a 1s window
      fps_frames += 1;
      let elapsed = now.wrapping_sub(fps_start);
      if elapsed >= 1000 {
        fps = fps_frames * 1000 / elapsed;
        fps_frames = 0;
        fps_start = now;
      }

      // Status bar (built without any formatting machinery)
      let mut status = [0u8; 40];
      let mut len = 0;
      fmt::append_str(&mut status, &mut len, scene_name(SCENES[scene_idx]));
      fmt::append_str(&mut status, &mut len, " f");
      fmt::append_u32(&mut status, &mut len, frame);
      fmt::append_str(&mut status, &mut len, " ");
      fmt::append_u32(&mut status, &mut len, fps);
      fmt::append_str(&mut status, &mut len, "fps");
      if paused {
        fmt::append_str(&mut status, &mut len, " PAUSED");
      }
      let status = core::str::from_utf8(&status[..len]).unwrap();
      canvas.fill_rect(Rect::new(0, canvas.height() as i32 - 12, canvas.width() as i32, 12), Rgb565::BLACK);
      canvas.draw_text(status, 4, canvas.height() as i32 - 11, Rgb565::WHITE, 1);

      unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };
      yield_now().await;
    }
  })());
}

fn draw_scene(canvas: &mut Canvas, scene: Scene, frame: u32) {
  match scene {
    Scene::Palette => draw_palette(canvas, frame),
    Scene::Lines => draw_lines(canvas, frame),
    Scene::Rects => draw_rects(canvas, frame),
    Scene::Circles => draw_circles(canvas, frame),
    Scene::Triangles => draw_triangles(canvas, frame),
    Scene::Text => draw_text_scene(canvas, frame),
    Scene::All => draw_all(canvas, frame),
  }
}

// ---------------------------------------------------------------------------
// Scene: palette + pixels
// ---------------------------------------------------------------------------

fn draw_palette(canvas: &mut Canvas, frame: u32) {
  canvas.clear(Rgb565::BLACK);

  let swatches: [(Rgb565, &str); 8] = [
    (Rgb565::WHITE, "WHT"),
    (Rgb565::RED, "RED"),
    (Rgb565::GREEN, "GRN"),
    (Rgb565::BLUE, "BLU"),
    (Rgb565::YELLOW, "YEL"),
    (Rgb565::CYAN, "CYN"),
    (Rgb565::MAGENTA, "MAG"),
    (Rgb565::ORANGE, "ORG"),
  ];
  let sw = 26;
  for (i, (color, name)) in swatches.iter().enumerate() {
    let x = 4 + i as i32 * 30;
    canvas.fill_rect(Rect::new(x, 6, sw, sw), *color);
    canvas.draw_text(name, x, 6 + sw + 3, *color, 1);
  }

  // 32-step gray ramp (from_rgb888)
  for i in 0..32 {
    let v = (i * 8) as u8;
    let c = Rgb565::from_rgb888(v, v, v);
    canvas.fill_rect(Rect::new(4 + i as i32 * 7, 56, 6, 10), c);
  }

  // R/G/B channel gradients
  for i in 0..64 {
    let v = (i * 4) as u8;
    canvas.fill_rect(Rect::new(4 + i as i32 * 3, 74, 3, 7), Rgb565::from_rgb888(v, 0, 0));
    canvas.fill_rect(Rect::new(4 + i as i32 * 3, 83, 3, 7), Rgb565::from_rgb888(0, v, 0));
    canvas.fill_rect(Rect::new(4 + i as i32 * 3, 92, 3, 7), Rgb565::from_rgb888(0, 0, v));
  }

  // Raw 5/6/5 component swatches (Rgb565::new)
  let raw = [
    Rgb565::new(31, 0, 0),   // max red
    Rgb565::new(0, 63, 0),   // max green
    Rgb565::new(0, 0, 31),   // max blue
    Rgb565::new(31, 63, 31), // all max
  ];
  for (i, color) in raw.iter().enumerate() {
    canvas.fill_rect(Rect::new(4 + i as i32 * 22, 102, 18, 10), *color);
  }

  // Animated per-pixel noise (deterministic per frame via the seed)
  let mut rng = XorShift64::new(0x5EED_CAFE ^ frame as u64);
  for y in 116..canvas.height() as i32 - 12 {
    for x in 0..canvas.width() as i32 {
      let v = (rng.next_u32() & 0xFF) as u8;
      let c = match v % 5 {
        0 => Rgb565::WHITE,
        1 => Rgb565::RED,
        2 => Rgb565::GREEN,
        3 => Rgb565::BLUE,
        _ => Rgb565::from_rgb888(v, 255 - v, v / 2),
      };
      canvas.set_pixel(x, y, c);
    }
  }
}

// ---------------------------------------------------------------------------
// Scene: lines
// ---------------------------------------------------------------------------

fn draw_lines(canvas: &mut Canvas, frame: u32) {
  canvas.clear(Rgb565::BLACK);

  // Grid
  canvas.draw_rect(Rect::new(0, 0, canvas.width() as i32, canvas.height() as i32), Rgb565::DARK_GRAY);
  for i in (24..canvas.width()).step_by(24) {
    canvas.draw_line(
      Point::new(i as i32, 0),
      Point::new(i as i32, canvas.height() as i32 - 1),
      Rgb565::DARK_GRAY,
    );
    canvas.draw_line(
      Point::new(0, i as i32),
      Point::new(canvas.width() as i32 - 1, i as i32),
      Rgb565::DARK_GRAY,
    );
  }

  // Rotating spokes
  let center = Point::new((canvas.width() / 2) as i32, (canvas.height() / 2) as i32);
  let radius = 108.0;
  let spokes = [
    (Rgb565::RED, 0),
    (Rgb565::YELLOW, 1),
    (Rgb565::GREEN, 2),
    (Rgb565::CYAN, 3),
    (Rgb565::BLUE, 4),
    (Rgb565::MAGENTA, 5),
  ];
  let base = frame as f32 * 0.02;
  let n = spokes.len() as f32;
  for (color, i) in spokes {
    let a = base + i as f32 * core::f32::consts::TAU / n;
    let p = Point::new(
      center.x + (crate::lib::trig::fast_cos(a) * radius) as i32,
      center.y + (crate::lib::trig::fast_sin(a) * radius) as i32,
    );
    canvas.draw_line(center, p, color);
  }

  // Sweeping diagonals
  let t = frame as i32 % (canvas.width() as i32 + 1);
  canvas.draw_line(Point::new(t, 0), Point::new(0, t), Rgb565::WHITE);
  canvas.draw_line(
    Point::new(canvas.width() as i32 - 1 - t, canvas.height() as i32 - 1),
    Point::new(canvas.width() as i32 - 1, canvas.height() as i32 - 1 - t),
    Rgb565::LIGHT_GRAY,
  );
}

// ---------------------------------------------------------------------------
// Scene: rectangles
// ---------------------------------------------------------------------------

fn draw_rects(canvas: &mut Canvas, frame: u32) {
  canvas.clear(Rgb565::BLACK);

  // Concentric outline rectangles with cycling colors
  for i in 0..20 {
    let inset = i * 6;
    if inset >= 120 {
      break;
    }
    let v = (frame as i32 + i as i32 * 10) & 0xFF;
    let c = Rgb565::from_rgb888(v as u8, (255 - v) as u8, ((v * 3) & 0xFF) as u8);
    canvas.draw_rect(
      Rect::new(inset, inset, canvas.width() as i32 - inset * 2, canvas.height() as i32 - inset * 2),
      c,
    );
  }

  // Ping-ponging filled rect with outline
  let span = 180;
  let t = frame as i32 % (span * 2);
  let x = if t < span { t } else { span * 2 - t };
  canvas.fill_rect(Rect::new(30 + x, 60, 20, 20), Rgb565::ORANGE);
  canvas.draw_rect(Rect::new(30 + x, 60, 20, 20), Rgb565::WHITE);

  // Progress bar
  let w = frame as i32 % 216 + 4;
  canvas.fill_rect(Rect::new(10, 200, w, 22), Rgb565::GREEN);
  canvas.draw_rect(Rect::new(10, 200, 220, 22), Rgb565::WHITE);
}

// ---------------------------------------------------------------------------
// Scene: circles
// ---------------------------------------------------------------------------

fn draw_circles(canvas: &mut Canvas, frame: u32) {
  canvas.clear(Rgb565::BLACK);

  let center = Point::new((canvas.width() / 2) as i32, (canvas.height() / 2) as i32);

  // Concentric outline circles
  for r in (8..=108).step_by(10) {
    let v = (frame as i32 + r as i32 * 5) & 0xFF;
    let c = Rgb565::from_rgb888((255 - v) as u8, v as u8, ((v * 2) & 0xFF) as u8);
    canvas.draw_circle(center, r, c);
  }

  // Grid of filled circles with random colors
  let mut rng = XorShift64::new(0xC0FFEE ^ frame as u64);
  let mut y = 8;
  while y < 120 {
    let mut x = 8;
    while x < canvas.width() as i32 {
      canvas.fill_circle(Point::new(x, y), 6, rng.color());
      x += 24;
    }
    y += 24;
  }

  // Bouncing ball (filled + outline)
  let span = 200;
  let t = frame as i32 % (span * 2);
  let bx = if t < span { t } else { span * 2 - t };
  let t2 = (frame as i32 / 2) % (span * 2);
  let by = if t2 < span { t2 } else { span * 2 - t2 };
  let ball = Point::new(20 + bx, 20 + by);
  canvas.draw_circle(ball, 16, Rgb565::WHITE);
  canvas.fill_circle(ball, 11, Rgb565::ORANGE);
}

// ---------------------------------------------------------------------------
// Scene: triangles
// ---------------------------------------------------------------------------

fn draw_triangles(canvas: &mut Canvas, frame: u32) {
  canvas.clear(Rgb565::BLACK);

  let center = Point::new((canvas.width() / 2) as i32, (canvas.height() / 2) as i32);
  let tau = core::f32::consts::TAU;

  // Rotating fan of filled triangles
  let base = frame as f32 * 0.02;
  let n = 12;
  let r = 104.0;
  for i in 0..n {
    let a0 = base + i as f32 * tau / n as f32;
    let a1 = base + (i + 1) as f32 * tau / n as f32;
    let p0 = Point::new(
      center.x + (crate::lib::trig::fast_cos(a0) * r) as i32,
      center.y + (crate::lib::trig::fast_sin(a0) * r) as i32,
    );
    let p1 = Point::new(
      center.x + (crate::lib::trig::fast_cos(a1) * r) as i32,
      center.y + (crate::lib::trig::fast_sin(a1) * r) as i32,
    );
    let v = (i as i32 * 25 + frame as i32 * 2) & 0xFF;
    canvas.fill_triangle(center, p0, p1, Rgb565::from_rgb888(v as u8, (255 - v) as u8, 128));
  }

  // Counter-rotating outline triangle
  let r2 = 56.0;
  let a = -base * 1.5;
  let t0 = Point::new(
    center.x + (crate::lib::trig::fast_cos(a) * r2) as i32,
    center.y + (crate::lib::trig::fast_sin(a) * r2) as i32,
  );
  let t1 = Point::new(
    center.x + (crate::lib::trig::fast_cos(a + tau / 3.0) * r2) as i32,
    center.y + (crate::lib::trig::fast_sin(a + tau / 3.0) * r2) as i32,
  );
  let t2 = Point::new(
    center.x + (crate::lib::trig::fast_cos(a + 2.0 * tau / 3.0) * r2) as i32,
    center.y + (crate::lib::trig::fast_sin(a + 2.0 * tau / 3.0) * r2) as i32,
  );
  canvas.draw_triangle(t0, t1, t2, Rgb565::WHITE);
}

// ---------------------------------------------------------------------------
// Scene: text
// ---------------------------------------------------------------------------

fn draw_text_scene(canvas: &mut Canvas, frame: u32) {
  canvas.clear(Rgb565::BLACK);

  centered_text(canvas, "5x7 bitmap font", 6, 1, Rgb565::GRAY);
  centered_text(canvas, "Rustagon", 24, 2, Rgb565::YELLOW);
  centered_text(canvas, "SHOWCASE", 52, 3, Rgb565::RED);

  // Multi-line + scale comparison
  canvas.draw_text("line 1/2 (2x)", 8, 100, Rgb565::WHITE, 2);
  canvas.draw_text("line 2/2 (1x)", 8, 100 + text_height("x", 2), Rgb565::CYAN, 1);

  // Scrolling marquee (2x)
  let marquee = "zero-dep gfx -> no embedded_graphics -> ";
  let tw = text_width(marquee, 2);
  let x = canvas.width() as i32 - ((frame as i32 * 3) % (tw + canvas.width() as i32));
  canvas.draw_text(marquee, x, 168, Rgb565::MAGENTA, 2);

  // Blinking cursor after "Rustagon"
  if (frame / 30) % 2 == 0 {
    let tw = text_width("Rustagon", 2);
    let cx = (canvas.width() as i32 - tw) / 2 + tw;
    canvas.fill_rect(Rect::new(cx, 24, 4, 16), Rgb565::WHITE);
  }
}

fn centered_text(canvas: &mut Canvas, text: &str, y: i32, scale: u8, color: Rgb565) {
  let x = (canvas.width() as i32 - text_width(text, scale)) / 2;
  canvas.draw_text(text, x, y, color, scale);
}

// ---------------------------------------------------------------------------
// Scene: everything
// ---------------------------------------------------------------------------

fn draw_all(canvas: &mut Canvas, frame: u32) {
  canvas.clear(Rgb565::DARK_GRAY);

  // Frame
  canvas.draw_rect(Rect::new(2, 2, 236, 236), Rgb565::WHITE);
  canvas.draw_rect(Rect::new(5, 5, 230, 230), Rgb565::GRAY);

  // Radial lines
  let center = Point::new((canvas.width() / 2) as i32, (canvas.height() / 2) as i32);
  for i in 0..8 {
    let a = frame as f32 * 0.01 + i as f32 * core::f32::consts::TAU / 8.0;
    let p = Point::new(
      center.x + (crate::lib::trig::fast_cos(a) * 88.0) as i32,
      center.y + (crate::lib::trig::fast_sin(a) * 88.0) as i32,
    );
    canvas.draw_line(center, p, Rgb565::YELLOW);
  }

  // Shapes
  canvas.draw_rect(Rect::new(24, 24, 36, 36), Rgb565::RED);
  canvas.fill_rect(Rect::new(180, 180, 36, 36), Rgb565::BLUE);
  canvas.draw_circle(center, 44, Rgb565::WHITE);
  canvas.fill_circle(center, 18, Rgb565::ORANGE);
  canvas.fill_triangle(Point::new(24, 176), Point::new(56, 176), Point::new(40, 206), Rgb565::GREEN);
  canvas.draw_triangle(Point::new(180, 24), Point::new(216, 24), Point::new(198, 54), Rgb565::CYAN);

  // read_pixel demo: sample the orange circle center, show its raw value
  if let Some(p) = canvas.read_pixel(120, 120) {
    let mut raw_buf = [0u8; 8];
    let raw = fmt::u32_to_hex(p.raw() as u32, &mut raw_buf);
    canvas.draw_text(raw, 8, 216, Rgb565::WHITE, 1);
  }

  // blit demo: 8x8 checker tile repeated
  let mut tile = [0u8; 8 * 8 * 2];
  for ty in 0..8 {
    for tx in 0..8 {
      let c = if (tx + ty) % 2 == 0 { Rgb565::WHITE } else { Rgb565::BLUE };
      let i = (ty * 8 + tx) * 2;
      tile[i] = (c.raw() >> 8) as u8;
      tile[i + 1] = (c.raw() & 0xFF) as u8;
    }
  }
  for row in 0..3 {
    for col in 0..3 {
      canvas.blit(16 + col as i32 * 24, 56 + row as i32 * 24, 8, 8, &tile);
    }
  }

  // Random sparkle pixels
  let mut rng = XorShift64::new(frame as u64);
  for _ in 0..24 {
    let x = (rng.next_u32() % 220) as i32 + 10;
    let y = (rng.next_u32() % 190) as i32 + 10;
    canvas.set_pixel(x, y, rng.color());
  }

  centered_text(canvas, "gfx demo", 120, 2, Rgb565::WHITE);
}
