// Flappy Bird — a minimal clone for the Rustagon badge.
//
// Controls:
//   Fire (or Up)  — flap
//   Fire          — start / restart
//
// Tap to keep the bird between the scrolling pipes. Each pipe passed scores
// a point. Hitting a pipe or the ground ends the game.

#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt::{append_str, append_u32},
  gfx::{text_width, Canvas, Point, Rect, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::get_millis,
  protocol::{extern_set_lcd_buffer, HexButton, HostIpcMessage},
  tasks::{spawn, yield_now, HOST_IPC_CHANNEL},
  trig::{fast_cos, fast_sin},
};
use alloc::{boxed::Box, vec::Vec};

// ── Geometry ────────────────────────────────────────────────────────
const SW: i32 = SCREEN_WIDTH as i32;
const SH: i32 = SCREEN_HEIGHT as i32;
const GROUND_Y: f32 = 208.0; // top of the ground strip

// ── Bird ────────────────────────────────────────────────────────────
const BIRD_X: f32 = 70.0;
const BIRD_R: f32 = 11.0;
const GRAVITY: f32 = 1000.0; // px/sec²
const FLAP_VY: f32 = -330.0; // px/sec
const MAX_FALL: f32 = 340.0; // px/sec

// ── Pipes ───────────────────────────────────────────────────────────
const PIPE_W: f32 = 36.0;
const GAP_H: f32 = 82.0; // vertical gap between pipes
const PIPE_SPEED: f32 = 105.0; // px/sec
const PIPE_SPACING: f32 = 165.0; // px between pipes
const PIPE_INTERVAL: u32 = (PIPE_SPACING / PIPE_SPEED * 1000.0) as u32; // ms
const FIRST_PIPE_DELAY: u32 = 900; // ms before the first pipe appears

// ── Colors ──────────────────────────────────────────────────────────
const SKY_TOP: Rgb565 = Rgb565::from_rgb888(90, 175, 240);
const SKY_MID: Rgb565 = Rgb565::from_rgb888(125, 200, 245);
const SKY_BOT: Rgb565 = Rgb565::from_rgb888(160, 220, 250);
const CLR_CLOUD: Rgb565 = Rgb565::from_rgb888(235, 245, 250);
const CLR_PIPE: Rgb565 = Rgb565::from_rgb888(80, 185, 80);
const CLR_PIPE_LIP: Rgb565 = Rgb565::from_rgb888(140, 230, 110);
const CLR_PIPE_EDGE: Rgb565 = Rgb565::from_rgb888(45, 135, 45);
const CLR_GROUND: Rgb565 = Rgb565::from_rgb888(185, 150, 60);
const CLR_GROUND_LINE: Rgb565 = Rgb565::from_rgb888(140, 112, 44);
const CLR_GRASS: Rgb565 = Rgb565::from_rgb888(100, 200, 80);
const CLR_BIRD: Rgb565 = Rgb565::from_rgb888(255, 210, 60);
const CLR_BIRD_BELLY: Rgb565 = Rgb565::from_rgb888(255, 235, 150);
const CLR_WING: Rgb565 = Rgb565::from_rgb888(235, 165, 30);
const CLR_BEAK: Rgb565 = Rgb565::from_rgb888(255, 120, 40);
const CLR_EYE_P: Rgb565 = Rgb565::from_rgb888(30, 30, 30);

// ── Game states ─────────────────────────────────────────────────────
const S_MENU: u8 = 0;
const S_PLAY: u8 = 1;
const S_GAMEOVER: u8 = 2;

// ── RNG (xorshift32, same as the shooter demo) ─────────────────────
struct Rng {
  state: u32,
}

impl Rng {
  fn new(seed: u32) -> Self {
    Rng { state: seed | 1 }
  }

  fn next(&mut self) -> u32 {
    let mut x = self.state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    self.state = x;
    x
  }

  fn range(&mut self, lo: i32, hi: i32) -> i32 {
    lo + (self.next() as i32 % (hi - lo + 1))
  }
}

// ── Entities ────────────────────────────────────────────────────────
struct Pipe {
  x: f32,     // left edge
  gap_y: f32, // center of the gap
  scored: bool,
}

struct Bird {
  y: f32,
  vy: f32,
}

impl Bird {
  fn reset(&mut self) {
    self.y = SH as f32 / 2.0;
    self.vy = 0.0;
  }

  fn flap(&mut self) {
    self.vy = FLAP_VY;
  }
}

struct Game {
  bird: Bird,
  pipes: Vec<Pipe>,
  rng: Rng,
  score: u32,
  best: u32,
  state: u8,
  pipe_timer: u32,
}

impl Game {
  fn new() -> Self {
    let seed = get_millis();
    Game {
      bird: Bird {
        y: SH as f32 / 2.0,
        vy: 0.0,
      },
      pipes: Vec::with_capacity(6),
      rng: Rng::new(seed),
      score: 0,
      best: 0,
      state: S_MENU,
      pipe_timer: FIRST_PIPE_DELAY,
    }
  }

  fn start(&mut self) {
    self.bird.reset();
    self.pipes.clear();
    self.score = 0;
    self.pipe_timer = FIRST_PIPE_DELAY;
    self.state = S_PLAY;
  }

  fn game_over(&mut self) {
    if self.score > self.best {
      self.best = self.score;
    }
    self.state = S_GAMEOVER;
  }

  fn spawn_pipe(&mut self) {
    let gap_y = self.rng.range(78, GROUND_Y as i32 - 55) as f32;
    self.pipes.push(Pipe {
      x: SW as f32,
      gap_y,
      scored: false,
    });
  }

  fn update(&mut self, delta: u32) {
    if self.state != S_PLAY {
      return;
    }
    let dt = delta as f32 * 0.001;

    // Bird physics
    self.bird.vy += GRAVITY * dt;
    if self.bird.vy > MAX_FALL {
      self.bird.vy = MAX_FALL;
    }
    self.bird.y += self.bird.vy * dt;

    // Clamp at the ceiling (no death), ground is fatal
    if self.bird.y < BIRD_R {
      self.bird.y = BIRD_R;
      self.bird.vy = 0.0;
    }
    if self.bird.y + BIRD_R >= GROUND_Y {
      self.bird.y = GROUND_Y - BIRD_R;
      self.game_over();
      return;
    }

    // Move pipes left
    for p in &mut self.pipes {
      p.x -= PIPE_SPEED * dt;
    }

    // Spawn a new pipe on a timer
    self.pipe_timer = self.pipe_timer.saturating_sub(delta);
    if self.pipe_timer == 0 {
      self.spawn_pipe();
      self.pipe_timer = PIPE_INTERVAL;
    }

    // Cull off-screen pipes
    self.pipes.retain(|p| p.x + PIPE_W > -10.0);

    // Scoring
    let bx = BIRD_X as f32;
    for p in &mut self.pipes {
      if !p.scored && p.x + PIPE_W < bx {
        p.scored = true;
        self.score += 1;
      }
    }

    // Pipe collision (circle vs the two pipe rectangles)
    let half = GAP_H / 2.0;
    for p in &self.pipes {
      let top_h = p.gap_y - half;
      let bot_y = p.gap_y + half;
      let hit = circle_rect_overlap(BIRD_X, self.bird.y, BIRD_R, p.x, 0.0, PIPE_W, top_h)
        || circle_rect_overlap(BIRD_X, self.bird.y, BIRD_R, p.x, bot_y, PIPE_W, GROUND_Y - bot_y);
      if hit {
        self.game_over();
        return;
      }
    }
  }
}

fn circle_rect_overlap(cx: f32, cy: f32, cr: f32, rx: f32, ry: f32, rw: f32, rh: f32) -> bool {
  if rh <= 0.0 {
    return false;
  }
  let nx = cx.max(rx).min(rx + rw);
  let ny = cy.max(ry).min(ry + rh);
  let dx = cx - nx;
  let dy = cy - ny;
  dx * dx + dy * dy <= cr * cr
}

// ── Rendering helpers ───────────────────────────────────────────────
/// Rotate an offset around `(cx, cy)` by `a` radians.
fn rot(cx: i32, cy: i32, dx: i32, dy: i32, a: f32) -> Point {
  let ca = fast_cos(a);
  let sa = fast_sin(a);
  Point::new(
    (cx as f32 + dx as f32 * ca - dy as f32 * sa) as i32,
    (cy as f32 + dx as f32 * sa + dy as f32 * ca) as i32,
  )
}

fn draw_sky(canvas: &mut Canvas) {
  canvas.fill_rect(Rect::new(0, 0, SW, 70), SKY_TOP);
  canvas.fill_rect(Rect::new(0, 70, SW, 70), SKY_MID);
  canvas.fill_rect(Rect::new(0, 140, SW, GROUND_Y as i32 - 140), SKY_BOT);
}

fn draw_cloud(canvas: &mut Canvas, x: i32, y: i32) {
  canvas.fill_circle(Point::new(x, y), 9, CLR_CLOUD);
  canvas.fill_circle(Point::new(x + 11, y + 2), 7, CLR_CLOUD);
  canvas.fill_circle(Point::new(x - 10, y + 2), 7, CLR_CLOUD);
  canvas.fill_circle(Point::new(x + 2, y - 4), 8, CLR_CLOUD);
}

fn draw_pipe(canvas: &mut Canvas, x: i32, gap_y: i32) {
  let half = (GAP_H / 2.0) as i32;
  let top_h = gap_y - half;
  let bot_y = gap_y + half;
  let gy = GROUND_Y as i32;

  // Top pipe body + lip
  canvas.fill_rect(Rect::new(x, 0, PIPE_W as i32, top_h), CLR_PIPE);
  canvas.fill_rect(Rect::new(x - 2, top_h - 8, PIPE_W as i32 + 4, 8), CLR_PIPE_LIP);
  canvas.draw_rect(Rect::new(x, 0, PIPE_W as i32, top_h), CLR_PIPE_EDGE);

  // Bottom pipe body + lip
  canvas.fill_rect(Rect::new(x, bot_y, PIPE_W as i32, gy - bot_y), CLR_PIPE);
  canvas.fill_rect(Rect::new(x - 2, bot_y, PIPE_W as i32 + 4, 8), CLR_PIPE_LIP);
  canvas.draw_rect(Rect::new(x, bot_y, PIPE_W as i32, gy - bot_y), CLR_PIPE_EDGE);
}

fn draw_ground(canvas: &mut Canvas, t_ms: u32) {
  let gy = GROUND_Y as i32;
  canvas.fill_rect(Rect::new(0, gy, SW, SH - gy), CLR_GROUND);
  canvas.fill_rect(Rect::new(0, gy, SW, 5), CLR_GRASS);

  // Scrolling diagonal stripes
  let off = ((t_ms as f32 * 0.06) as i32) % 32;
  let mut x = -off;
  while x < SW + 32 {
    canvas.draw_line(Point::new(x, gy + 4), Point::new(x + 24, SH), CLR_GROUND_LINE);
    x += 32;
  }
}

fn draw_bird(canvas: &mut Canvas, x: i32, y: i32, angle: f32, wing_t: f32) {
  // Body + belly
  canvas.fill_circle(Point::new(x, y), 11, CLR_BIRD);
  canvas.fill_circle(Point::new(x, y + 2), 7, CLR_BIRD_BELLY);

  // Flapping wing
  let wing = rot(x, y, -3, 1, angle + fast_sin(wing_t * 18.0) * 0.55);
  canvas.fill_circle(wing, 5, CLR_WING);

  // Eye (rotated with the body)
  let e = rot(x, y, 4, -4, angle);
  canvas.fill_circle(e, 4, Rgb565::WHITE);
  canvas.fill_circle(Point::new(e.x + 1, e.y), 2, CLR_EYE_P);

  // Beak (rotated with the body)
  let b1 = rot(x, y, 11, -1, angle);
  let b2 = rot(x, y, 16, 1, angle);
  let b3 = rot(x, y, 11, 3, angle);
  canvas.fill_triangle(b1, b2, b3, CLR_BEAK);
}

fn draw_score(canvas: &mut Canvas, score: u32, y: i32, scale: u8) {
  let mut buf = [0u8; 32];
  let mut len = 0;
  append_str(&mut buf, &mut len, "SCORE ");
  append_u32(&mut buf, &mut len, score);
  let text = core::str::from_utf8(&buf[..len]).unwrap();
  let x = (SW - text_width(text, scale)) / 2;
  canvas.draw_text(text, x + 1, y + 1, Rgb565::BLACK, scale);
  canvas.draw_text(text, x, y, Rgb565::WHITE, scale);
}

// ── Entry point ─────────────────────────────────────────────────────
#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async move || {
    let mut buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);
    let mut canvas = Canvas::new(&mut buf[..], SCREEN_WIDTH, SCREEN_HEIGHT);

    let mut game = Game::new();
    let mut subscriber = log_error!(HOST_IPC_CHANNEL.subscriber(), "flappy: subscriber");
    let mut last_frame = get_millis();

    loop {
      let now = get_millis();
      let delta = now.wrapping_sub(last_frame);
      last_frame = now;
      // Cap delta to avoid physics explosions on the first frame
      let delta = if delta > 50 { 50 } else { delta };
      let t = now as f32 * 0.001;

      // ── Input ──────────────────────────────────────────────
      loop {
        if let Some((_, msg)) = subscriber.try_next_message_pure() {
          if let HostIpcMessage::HexButton(btn) = msg {
            match btn {
              HexButton::Fire | HexButton::Up => match game.state {
                S_MENU | S_GAMEOVER => game.start(),
                _ => game.bird.flap(),
              },
              _ => {}
            }
          }
        } else {
          break;
        }
      }

      // ── Update ─────────────────────────────────────────────
      game.update(delta);

      // ── Render ─────────────────────────────────────────────
      draw_sky(&mut canvas);

      // Drifting clouds (two, wrapped around the screen)
      let span = SW + 140;
      let c1 = ((t * 8.0) as i32).rem_euclid(span) - 70;
      let c2 = ((t * 5.0 + 120.0) as i32).rem_euclid(span) - 70;
      draw_cloud(&mut canvas, c1, 46);
      draw_cloud(&mut canvas, c2, 118);

      for p in &game.pipes {
        draw_pipe(&mut canvas, p.x as i32, p.gap_y as i32);
      }

      draw_ground(&mut canvas, now);

      // Bird (bobs gently on the menu / game over screens)
      let (by, angle) = match game.state {
        S_PLAY => {
          let a = (game.bird.vy / MAX_FALL).clamp(-1.0, 1.0) * 1.0;
          (game.bird.y as i32, a)
        }
        S_GAMEOVER => {
          let y = (game.bird.y as i32).min(GROUND_Y as i32 - 14);
          (y, 1.15)
        }
        _ => (SH as i32 / 2 + (fast_sin(t * 2.5) * 8.0) as i32, -0.25),
      };
      draw_bird(&mut canvas, BIRD_X as i32, by, angle, t);

      match game.state {
        S_MENU => {
          draw_score(&mut canvas, game.best, 14, 2);
          canvas.draw_text("FLAPPY BIRD", (SW - text_width("FLAPPY BIRD", 2)) / 2, 70, Rgb565::WHITE, 2);
          canvas.draw_text(
            "FIRE / UP TO FLAP",
            (SW - text_width("FIRE / UP TO FLAP", 1)) / 2,
            100,
            Rgb565::LIGHT_GRAY,
            1,
          );
          canvas.draw_text(
            "PRESS FIRE TO START",
            (SW - text_width("PRESS FIRE TO START", 1)) / 2,
            160,
            Rgb565::YELLOW,
            1,
          );
        }
        S_PLAY => {
          draw_score(&mut canvas, game.score, 14, 2);
        }
        S_GAMEOVER => {
          draw_score(&mut canvas, game.best, 14, 2);
          canvas.draw_text("GAME OVER", (SW - text_width("GAME OVER", 2)) / 2, 80, Rgb565::RED, 2);
          let mut buf = [0u8; 32];
          let mut len = 0;
          append_str(&mut buf, &mut len, "SCORE ");
          append_u32(&mut buf, &mut len, game.score);
          let text = core::str::from_utf8(&buf[..len]).unwrap();
          canvas.draw_text(text, (SW - text_width(text, 1)) / 2, 105, Rgb565::WHITE, 1);
          canvas.draw_text("PRESS FIRE", (SW - text_width("PRESS FIRE", 1)) / 2, 130, Rgb565::YELLOW, 1);
        }
        _ => {}
      }

      unsafe {
        extern_set_lcd_buffer(canvas.as_ptr());
      }

      yield_now().await;
    }
  })());
}
