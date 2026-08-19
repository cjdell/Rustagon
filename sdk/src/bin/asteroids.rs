// Asteroids — a minimal Asteroids clone for the Rustagon badge.
//
// Controls:
//   Left  / Right  — rotate the ship
//   Up              — thrust forward
//   Fire            — shoot
//
// The ship and asteroids wrap around the screen edges (toroidal geometry).
// Destroy all asteroids to advance; avoid collisions with drifting rocks.
// The ship flashes with 1.5 s of invincibility after each hit.

#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt::{append_str, append_u32},
  gfx::{Canvas, Point, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::get_millis,
  protocol::{HexButton, HostIpcMessage, extern_set_lcd_buffer},
  tasks::{HOST_IPC_CHANNEL, spawn, yield_now},
  trig::{fast_cos, fast_sin, fast_sqrt},
};
use alloc::boxed::Box;
use alloc::vec::Vec;

// ── Math constants ──────────────────────────────────────────────
const TWO_PI: f32 = 6.2831853;
const HALF_PI: f32 = 1.5707963;
const PI: f32 = 3.1415927;

// ── Ship physics ────────────────────────────────────────────────
const SHIP_RADIUS: f32 = 10.0;
const THRUST: f32 = 90.0; // px/sec²
const FRICTION: f32 = 0.35; // per second
const MAX_SPEED: f32 = 140.0; // px/sec
const ROT_SPEED: f32 = 4.0; // rad/sec
const INVULN_MS: u32 = 1500; // 1.5 s grace period after hit

// ── Weapons ─────────────────────────────────────────────────────
const BULLET_SPEED: f32 = 200.0; // px/sec
const BULLET_TTL: u32 = 2000; // ms
const FIRE_CD: u32 = 250; // ms between shots

// ── Asteroids ───────────────────────────────────────────────────
const AST_LARGE: f32 = 18.0;
const AST_MED: f32 = 11.0;
const AST_SMALL: f32 = 6.5;
const NUM_LARGE: usize = 4;

// ── Background ──────────────────────────────────────────────────
const NUM_STARS: usize = 40;

// ── Game states ─────────────────────────────────────────────────
const S_MENU: u8 = 0;
const S_PLAY: u8 = 1;
const S_GAMEOVER: u8 = 2;

// ── RNG (simple LCG, same pattern as the Tetris demo) ──────────
struct Rng {
  state: u32,
}
impl Rng {
  fn next(&mut self) -> u32 {
    self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
    self.state
  }
  fn range(&mut self, n: usize) -> usize {
    self.next() as usize % n
  }
  fn float(&mut self) -> f32 {
    (self.next() >> 8) as f32 / ((1u32 << 24) as f32 - 1.0)
  }
}

// ── Helpers ─────────────────────────────────────────────────────
fn wrap(v: f32, max: f32) -> f32 {
  let mut r = v % max;
  if r < 0.0 {
    r += max;
  }
  r
}

fn dist_sq(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
  let dx = ax - bx;
  let dy = ay - by;
  dx * dx + dy * dy
}

// ── Ship ────────────────────────────────────────────────────────
struct Ship {
  x: f32,
  y: f32,
  vx: f32,
  vy: f32,
  angle: f32, // radians; 0 = right, -PI/2 = up
  radius: f32,
  rot: i32, // -1 = left, 0 = still, 1 = right
  thrusting: bool,
  fire_cd: u32, // ms until next shot
  invuln: u32,  // ms of invincibility remaining
}

impl Ship {
  fn new() -> Self {
    Ship {
      x: SCREEN_WIDTH as f32 / 2.0,
      y: SCREEN_HEIGHT as f32 / 2.0,
      vx: 0.0,
      vy: 0.0,
      angle: -HALF_PI,
      radius: SHIP_RADIUS,
      rot: 0,
      thrusting: false,
      fire_cd: 0,
      invuln: 0,
    }
  }

  fn reset(&mut self) {
    self.x = SCREEN_WIDTH as f32 / 2.0;
    self.y = SCREEN_HEIGHT as f32 / 2.0;
    self.vx = 0.0;
    self.vy = 0.0;
    self.angle = -HALF_PI;
    self.rot = 0;
    self.thrusting = false;
    self.fire_cd = 0;
    self.invuln = INVULN_MS;
  }

  fn update(&mut self, delta: u32) {
    let dt = delta as f32 * 0.001;

    // Rotation
    self.angle += self.rot as f32 * ROT_SPEED * dt;

    // Thrust — apply acceleration in the ship's facing direction
    if self.thrusting {
      let (dx, dy) = (fast_cos(self.angle), fast_sin(self.angle));
      self.vx += dx * THRUST * dt;
      self.vy += dy * THRUST * dt;
    }

    // Linear friction (exponential decay)
    let f = 1.0 - FRICTION * dt;
    self.vx *= f;
    self.vy *= f;

    // Cap speed
    let spd = fast_sqrt(self.vx * self.vx + self.vy * self.vy);
    if spd > MAX_SPEED {
      let r = MAX_SPEED / spd;
      self.vx *= r;
      self.vy *= r;
    }

    // Integrate position
    self.x += self.vx * dt;
    self.y += self.vy * dt;

    // Wrap around screen (toroidal)
    self.x = wrap(self.x, SCREEN_WIDTH as f32);
    self.y = wrap(self.y, SCREEN_HEIGHT as f32);

    // Decrement timers
    self.invuln = self.invuln.saturating_sub(delta);
    self.fire_cd = self.fire_cd.saturating_sub(delta);
  }

  fn try_fire(&mut self, bullets: &mut Vec<Bullet>) {
    if self.fire_cd > 0 {
      return;
    }
    self.fire_cd = FIRE_CD;
    let (dx, dy) = (fast_cos(self.angle), fast_sin(self.angle));
    let offset = self.radius * 1.5;
    bullets.push(Bullet {
      x: self.x + dx * offset,
      y: self.y + dy * offset,
      vx: dx * BULLET_SPEED,
      vy: dy * BULLET_SPEED,
      ttl: BULLET_TTL,
    });
  }

  fn draw(&self, canvas: &mut Canvas) {
    let r = self.radius;
    let (dx, dy) = (fast_cos(self.angle), fast_sin(self.angle));

    // Flash gray during invincibility (every 100 ms, 50 ms on / 50 ms off)
    let color = if self.invuln > 0 && (self.invuln / 100) % 2 == 1 {
      Rgb565::GRAY
    } else {
      Rgb565::WHITE
    };

    // Ship body — filled triangle pointing at `angle`
    let nose = Point::new((self.x + r * dx) as i32, (self.y + r * dy) as i32);
    let left = Point::new(
      (self.x + r * 0.75 * fast_cos(self.angle + 2.4)) as i32,
      (self.y + r * 0.75 * fast_sin(self.angle + 2.4)) as i32,
    );
    let right = Point::new(
      (self.x + r * 0.75 * fast_cos(self.angle - 2.4)) as i32,
      (self.y + r * 0.75 * fast_sin(self.angle - 2.4)) as i32,
    );
    canvas.fill_triangle(nose, left, right, color);

    // Thrust flame (visible when accelerating)
    if self.thrusting {
      let flame = Rgb565::from_rgb888(255, 140, 0);
      let back_angle = self.angle + PI;
      let p1 = Point::new(
        (self.x + r * 0.55 * fast_cos(back_angle)) as i32,
        (self.y + r * 0.55 * fast_sin(back_angle)) as i32,
      );
      let p2 = Point::new(
        (self.x + r * 0.9 * fast_cos(self.angle + 2.2)) as i32,
        (self.y + r * 0.9 * fast_sin(self.angle + 2.2)) as i32,
      );
      let p3 = Point::new(
        (self.x + r * 0.9 * fast_cos(self.angle - 2.2)) as i32,
        (self.y + r * 0.9 * fast_sin(self.angle - 2.2)) as i32,
      );
      canvas.fill_triangle(p1, p2, p3, flame);
    }
  }
}

// ── Bullet ──────────────────────────────────────────────────────
struct Bullet {
  x: f32,
  y: f32,
  vx: f32,
  vy: f32,
  ttl: u32, // ms remaining
}

// ── Asteroid ────────────────────────────────────────────────────
struct Asteroid {
  x: f32,
  y: f32,
  vx: f32,
  vy: f32,
  radius: f32,
  size: u8, // 0 = large, 1 = medium, 2 = small
}

impl Asteroid {
  fn new(rng: &mut Rng, x: f32, y: f32, vx: f32, vy: f32, size: u8) -> Self {
    let radius = match size {
      0 => AST_LARGE,
      1 => AST_MED,
      _ => AST_SMALL,
    };
    Asteroid {
      x,
      y,
      vx,
      vy,
      radius,
      size,
    }
  }

  fn update(&mut self, dt: f32) {
    self.x += self.vx * dt;
    self.y += self.vy * dt;
    self.x = wrap(self.x, SCREEN_WIDTH as f32);
    self.y = wrap(self.y, SCREEN_HEIGHT as f32);
  }

  fn draw(&self, canvas: &mut Canvas) {
    let cx = self.x as i32;
    let cy = self.y as i32;
    let r = self.radius as i32;
    let fill = Rgb565::from_rgb888(100, 100, 100);
    let edge = Rgb565::from_rgb888(170, 170, 170);

    canvas.fill_circle(Point::new(cx, cy), r, fill);
    canvas.draw_circle(Point::new(cx, cy), r, edge);

    // A couple of notches for character (deterministic per position)
    let notch_angle = (self.x * 0.3 + self.y * 0.17) as i32 as f32;
    for i in 0..3i32 {
      let a = notch_angle + (TWO_PI / 3.0) * i as f32;
      let inner = r as f32 * 0.4;
      let px1 = (cx as f32 + fast_cos(a) * inner) as i32;
      let py1 = (cy as f32 + fast_sin(a) * inner) as i32;
      let px2 = (cx as f32 + fast_cos(a + 0.3) * (r as f32 + 1.0)) as i32;
      let py2 = (cy as f32 + fast_sin(a + 0.3) * (r as f32 + 1.0)) as i32;
      canvas.draw_line(Point::new(px1, py1), Point::new(px2, py2), edge);
    }
  }
}

// ── Game state ──────────────────────────────────────────────────
struct GameState {
  ship: Ship,
  asteroids: Vec<Asteroid>,
  bullets: Vec<Bullet>,
  stars: [(i32, i32); NUM_STARS],
  rng: Rng,
  score: u32,
  level: u32,
  lives: u32,
  state: u8,
}

impl GameState {
  fn new() -> Self {
    let seed = get_millis();
    let mut rng = Rng { state: seed };

    // Deterministic star field
    let mut stars = [(0i32, 0i32); NUM_STARS];
    for i in 0..NUM_STARS {
      stars[i] = (rng.range(SCREEN_WIDTH) as i32, rng.range(SCREEN_HEIGHT) as i32);
    }

    let mut gs = GameState {
      ship: Ship::new(),
      asteroids: Vec::new(),
      bullets: Vec::new(),
      stars,
      rng,
      score: 0,
      level: 0,
      lives: 3,
      state: S_MENU,
    };
    gs.spawn_asteroids(NUM_LARGE);
    gs
  }

  fn spawn_asteroids(&mut self, count: usize) {
    self.asteroids.clear();
    let base_speed = 20.0 + 5.0 * self.level as f32;
    for _ in 0..count {
      let x = self.rng.float() * SCREEN_WIDTH as f32;
      let y = self.rng.float() * SCREEN_HEIGHT as f32;
      let angle = self.rng.float() * TWO_PI;
      let spd = base_speed + self.rng.float() * 15.0;
      self.asteroids.push(Asteroid::new(
        &mut self.rng,
        x,
        y,
        fast_cos(angle) * spd,
        fast_sin(angle) * spd,
        0, // large
      ));
    }
  }

  fn start_game(&mut self) {
    self.ship.reset();
    self.bullets.clear();
    self.spawn_asteroids(NUM_LARGE);
    self.score = 0;
    self.level = 0;
    self.lives = 3;
    self.state = S_PLAY;
  }

  fn draw_stars(&self, canvas: &mut Canvas) {
    for &(x, y) in &self.stars {
      canvas.set_pixel(x, y, Rgb565::WHITE);
    }
  }

  fn draw_hud(&self, canvas: &mut Canvas) {
    let mut buf = [0u8; 48];
    let mut len: usize;

    // Score
    len = 0;
    append_str(&mut buf, &mut len, "SCORE ");
    append_u32(&mut buf, &mut len, self.score);
    let text = core::str::from_utf8(&buf[..len]).unwrap();
    canvas.draw_text(text, 5, 5, Rgb565::WHITE, 1);

    // Lives
    len = 0;
    append_str(&mut buf, &mut len, "LIVES ");
    append_u32(&mut buf, &mut len, self.lives);
    let text = core::str::from_utf8(&buf[..len]).unwrap();
    canvas.draw_text(text, 180, 5, Rgb565::WHITE, 1);
  }
}

// ── Entry point ─────────────────────────────────────────────────
#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async move || {
    let mut buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);
    let mut canvas = Canvas::new(&mut buf[..], SCREEN_WIDTH, SCREEN_HEIGHT);
    let mut state = GameState::new();
    let mut subscriber = log_error!(HOST_IPC_CHANNEL.subscriber(), "asteroids: subscriber");
    let mut last_frame = get_millis();

    loop {
      let now = get_millis();
      let delta = now.wrapping_sub(last_frame);
      last_frame = now;
      // Cap delta to avoid physics explosions on the first frame
      let delta = if delta > 50 { 50 } else { delta };
      let dt = delta as f32 * 0.001;

      // ── Input ──────────────────────────────────────────────
      loop {
        if let Some((_, msg)) = subscriber.try_next_message_pure() {
          if let HostIpcMessage::HexButton(btn) = msg {
            if state.state != S_PLAY {
              // Any button starts the game from menu / game over
              state.start_game();
            } else {
              match btn {
                HexButton::Left => state.ship.rot = -1,
                HexButton::LeftReleased => {
                  if state.ship.rot == -1 {
                    state.ship.rot = 0;
                  }
                }
                HexButton::Right => state.ship.rot = 1,
                HexButton::RightReleased => {
                  if state.ship.rot == 1 {
                    state.ship.rot = 0;
                  }
                }
                HexButton::Up => state.ship.thrusting = true,
                HexButton::UpReleased => state.ship.thrusting = false,
                HexButton::Fire => state.ship.try_fire(&mut state.bullets),
                _ => {}
              }
            }
          }
        } else {
          break;
        }
      }

      // ── Update (only while playing) ────────────────────────
      if state.state == S_PLAY {
        // Ship
        state.ship.update(delta);

        // Asteroids
        for ast in &mut state.asteroids {
          ast.update(dt);
        }

        // Bullets — update lifetime, position, and cull off-screen
        let mut i = 0;
        while i < state.bullets.len() {
          let b = &mut state.bullets[i];
          b.ttl = b.ttl.saturating_sub(delta);
          if b.ttl == 0 {
            state.bullets.swap_remove(i);
            continue;
          }
          b.x += b.vx * dt;
          b.y += b.vy * dt;
          if b.x < -5.0 || b.x > SCREEN_WIDTH as f32 + 5.0 || b.y < -5.0 || b.y > SCREEN_HEIGHT as f32 + 5.0 {
            state.bullets.swap_remove(i);
            continue;
          }
          i += 1;
        }

        // Bullet-asteroid collision
        let mut bi = 0;
        while bi < state.bullets.len() {
          let mut hit = None;
          for (ai, ast) in state.asteroids.iter().enumerate() {
            if dist_sq(state.bullets[bi].x, state.bullets[bi].y, ast.x, ast.y) < ast.radius * ast.radius {
              hit = Some(ai);
              break;
            }
          }
          if let Some(ai) = hit {
            let ast = state.asteroids.swap_remove(ai);
            // Score
            state.score += match ast.size {
              0 => 100,
              1 => 50,
              _ => 20,
            };
            // Split into two smaller asteroids
            if ast.size < 2 {
              let sp = 25.0 + state.rng.float() * 20.0;
              for _ in 0..2 {
                let a = state.rng.float() * TWO_PI;
                state.asteroids.push(Asteroid::new(
                  &mut state.rng,
                  ast.x,
                  ast.y,
                  ast.vx + fast_cos(a) * sp,
                  ast.vy + fast_sin(a) * sp,
                  ast.size + 1,
                ));
              }
            }
            state.bullets.swap_remove(bi);
          } else {
            bi += 1;
          }
        }

        // Ship-asteroid collision (only when not invincible)
        if state.ship.invuln == 0 {
          let mut hit = false;
          for ast in &state.asteroids {
            let r = ast.radius + state.ship.radius;
            if dist_sq(state.ship.x, state.ship.y, ast.x, ast.y) < r * r {
              hit = true;
              break;
            }
          }
          if hit {
            state.lives -= 1;
            if state.lives > 0 {
              state.ship.reset();
              state.bullets.clear();
            } else {
              state.state = S_GAMEOVER;
            }
          }
        }

        // Level complete — all asteroids destroyed
        if state.asteroids.is_empty() {
          state.level += 1;
          state.spawn_asteroids(NUM_LARGE + state.level as usize);
        }
      }

      // ── Render ──────────────────────────────────────────────
      canvas.clear(Rgb565::BLACK);
      state.draw_stars(&mut canvas);

      if state.state == S_PLAY {
        for ast in &state.asteroids {
          ast.draw(&mut canvas);
        }
        for b in &state.bullets {
          canvas.fill_circle(Point::new(b.x as i32, b.y as i32), 2, Rgb565::WHITE);
        }
        state.ship.draw(&mut canvas);
        state.draw_hud(&mut canvas);
      } else if state.state == S_MENU {
        // Title screen
        canvas.draw_text("ASTEROIDS", 66, 55, Rgb565::WHITE, 2);
        canvas.draw_text("LEFT/RIGHT ROTATE", 36, 100, Rgb565::LIGHT_GRAY, 1);
        canvas.draw_text("UP THRUST", 60, 112, Rgb565::LIGHT_GRAY, 1);
        canvas.draw_text("FIRE SHOOT", 60, 124, Rgb565::LIGHT_GRAY, 1);
        canvas.draw_text("PRESS ANY BUTTON", 36, 165, Rgb565::YELLOW, 1);
      } else {
        // Game over
        canvas.draw_text("GAME OVER", 66, 75, Rgb565::RED, 2);
        let mut buf = [0u8; 32];
        let mut len: usize;
        len = 0;
        append_str(&mut buf, &mut len, "SCORE ");
        append_u32(&mut buf, &mut len, state.score);
        let text = core::str::from_utf8(&buf[..len]).unwrap();
        canvas.draw_text(text, 90, 110, Rgb565::WHITE, 1);
        canvas.draw_text("PRESS ANY BUTTON", 36, 160, Rgb565::YELLOW, 1);
      }

      unsafe {
        extern_set_lcd_buffer(canvas.as_ptr());
      }

      yield_now().await;
    }
  })());
}
