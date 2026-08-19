// A top-down 2D space shooter for the Rustagon badge.
//
// Controls:
//   Left  / Right  — strafe the ship horizontally
//   Up              — nudge upward (out of the danger zone)
//   Down            — nudge downward (return to cover)
//   Fire            — shoot upward
//
// Waves of enemies descend from the top. Destroy them before they reach the
// green "safe zone" at the bottom. Each destroyed enemy scores points; surviving
// the wave advances to the next level with faster, tougher foes.

#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt::{append_str, append_u32},
  gfx::{Canvas, Point, Rect, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::{get_millis, print_line},
  protocol::{HexButton, HostIpcMessage, extern_set_lcd_buffer},
  tasks::{HOST_IPC_CHANNEL, spawn, yield_now},
};
use alloc::boxed::Box;
use alloc::vec::Vec;

// ── Geometry ────────────────────────────────────────────────────────
const SW: i32 = SCREEN_WIDTH as i32;
const SH: i32 = SCREEN_HEIGHT as i32;

// ── Colors ──────────────────────────────────────────────────────────
const CLR_BG: Rgb565 = Rgb565::BLACK;
const CLR_SHIP: Rgb565 = Rgb565::from_rgb888(80, 200, 255);
const CLR_SHIP_LIGHT: Rgb565 = Rgb565::from_rgb888(160, 230, 255);
const CLR_BULLET: Rgb565 = Rgb565::YELLOW;
const CLR_ENEMY_S: Rgb565 = Rgb565::from_rgb888(200, 200, 200); // small — white/grey
const CLR_ENEMY_M: Rgb565 = Rgb565::from_rgb888(255, 120, 60); // medium — orange
const CLR_ENEMY_H: Rgb565 = Rgb565::from_rgb888(255, 80, 80); // heavy — red
const CLR_ENEMY_S2: Rgb565 = Rgb565::from_rgb888(180, 255, 100); // small tier-2 — green
const CLR_HUD: Rgb565 = Rgb565::WHITE;

// ── Ship ────────────────────────────────────────────────────────────
const SHIP_W: f32 = 14.0;
const SHIP_H: f32 = 20.0;
const SHIP_RADIUS: f32 = 10.0;
const SHIP_SPEED: f32 = 130.0; // px/sec
const SHIP_FIRE_CD: u32 = 240; // ms between shots

// ── Bullet ──────────────────────────────────────────────────────────
const BULLET_SPEED: f32 = 190.0;
const BULLET_W: f32 = 3.0;
const BULLET_H: f32 = 7.0;

// ── Enemies ─────────────────────────────────────────────────────────
const ENEMY_S_RADIUS: f32 = 7.0;
const ENEMY_M_RADIUS: f32 = 10.0;
const ENEMY_H_RADIUS: f32 = 13.0;

// ── Starfield ───────────────────────────────────────────────────────
const NUM_STARS: usize = 48;

// ── RNG ─────────────────────────────────────────────────────────────
struct Rng {
  state: u32,
}

impl Rng {
  fn new(seed: u32) -> Self {
    Rng { state: seed | 1 }
  }

  fn next(&mut self) -> u32 {
    // xorshift32
    let mut x = self.state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    self.state = x;
    x
  }

  fn range(&mut self, n: usize) -> usize {
    (self.next() as usize) % n
  }

  fn float(&mut self) -> f32 {
    (self.next() >> 8) as f32 / ((1u32 << 24) as f32 - 1.0)
  }
}

// ── Ship ────────────────────────────────────────────────────────────
struct Ship {
  x: f32,
  y: f32,
  vx: f32,
  target_y: f32, // the y position the ship is nudging toward
  fire_cd: u32,
}

impl Ship {
  fn new() -> Self {
    Ship {
      x: SW as f32 / 2.0,
      y: SH as f32 - 40.0,
      vx: 0.0,
      target_y: SH as f32 - 40.0,
      fire_cd: 0,
    }
  }

  fn reset(&mut self) {
    self.x = SW as f32 / 2.0;
    self.y = SH as f32 - 40.0;
    self.vx = 0.0;
    self.target_y = SH as f32 - 40.0;
    self.fire_cd = 0;
  }

  fn update(&mut self, delta: u32, left: bool, right: bool, up: bool, down: bool) {
    let dt = delta as f32 * 0.001;

    // Horizontal movement
    self.vx = 0.0;
    if left {
      self.vx -= SHIP_SPEED;
    }
    if right {
      self.vx += SHIP_SPEED;
    }
    self.x += self.vx * dt;
    if self.x < SHIP_RADIUS {
      self.x = SHIP_RADIUS;
    }
    if self.x > SW as f32 - SHIP_RADIUS {
      self.x = SW as f32 - SHIP_RADIUS;
    }

    // Vertical nudge (limited range)
    let min_y = SH as f32 - 70.0;
    let max_y = SH as f32 - 20.0;
    if up {
      self.target_y = min_y;
    } else if down {
      self.target_y = max_y;
    } else {
      self.target_y = max_y;
    }
    // Smooth lerp toward target y
    let dy = self.target_y - self.y;
    self.y += dy * (dt * 8.0).min(1.0);

    // Fire cooldown
    self.fire_cd = self.fire_cd.saturating_sub(delta);
  }

  fn try_fire(&mut self, bullets: &mut Vec<Bullet>) {
    if self.fire_cd > 0 {
      return;
    }
    self.fire_cd = SHIP_FIRE_CD;
    bullets.push(Bullet {
      x: self.x,
      y: self.y - SHIP_H,
      vx: 0.0,
      vy: -BULLET_SPEED,
      w: BULLET_W,
      h: BULLET_H,
      ttl: 1200,
    });
    // Slight spread on double-shot when holding is handled by fire_cd timing —
    // a second bullet at a small angle
    bullets.push(Bullet {
      x: self.x,
      y: self.y - SHIP_H,
      vx: -18.0,
      vy: -BULLET_SPEED * 0.93,
      w: BULLET_W,
      h: BULLET_H,
      ttl: 1200,
    });
    bullets.push(Bullet {
      x: self.x,
      y: self.y - SHIP_H,
      vx: 18.0,
      vy: -BULLET_SPEED * 0.93,
      w: BULLET_W,
      h: BULLET_H,
      ttl: 1200,
    });
  }

  fn draw(&self, canvas: &mut Canvas) {
    // Main body — a rounded triangle / chevron pointing up
    let cx = self.x as i32;
    let cy = self.y as i32;
    let r = SHIP_RADIUS as i32;

    // Wings
    let wing_l = Point::new(cx - 10, cy + 6);
    let wing_r = Point::new(cx + 10, cy + 6);
    let body_t = Point::new(cx, cy - r);
    let body_l = Point::new(cx - 7, cy + 4);
    let body_r = Point::new(cx + 7, cy + 4);

    // Ship body (filled polygon via two triangles)
    canvas.fill_triangle(body_t, body_r, Point::new(cx, cy + 2), CLR_SHIP);
    canvas.fill_triangle(body_t, Point::new(cx, cy + 2), body_l, CLR_SHIP);
    // Wings
    canvas.fill_triangle(wing_l, wing_r, Point::new(cx, cy + 2), CLR_SHIP);
    // Wing tips
    canvas.fill_circle(wing_l, 3, CLR_SHIP_LIGHT);
    canvas.fill_circle(wing_r, 3, CLR_SHIP_LIGHT);
    // Cockpit
    canvas.fill_circle(Point::new(cx, cy - 4), 4, CLR_SHIP_LIGHT);
  }
}

// ── Bullet ──────────────────────────────────────────────────────────
struct Bullet {
  x: f32,
  y: f32,
  vx: f32,
  vy: f32,
  w: f32,
  h: f32,
  ttl: u32,
}

// ── Enemy ───────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum EnemyType {
  Small,  // 1 hit, fast, white
  Medium, // 2 hits, orange
  Heavy,  // 3 hits, red, slow
  Small2, // 1 hit, green, fast & zigzags
}

struct Enemy {
  x: f32,
  y: f32,
  vx: f32,
  vy: f32,
  radius: f32,
  etype: EnemyType,
  hp: u8,
  zigzag_phase: f32,
}

impl Enemy {
  fn new_small(rng: &mut Rng, x: f32, y: f32, vx: f32, speed: f32) -> Self {
    Enemy {
      x,
      y,
      vx,
      vy: speed,
      radius: ENEMY_S_RADIUS,
      etype: EnemyType::Small,
      hp: 1,
      zigzag_phase: rng.float() * 6.2831853,
    }
  }

  fn new_medium(rng: &mut Rng, x: f32, y: f32, vx: f32, speed: f32) -> Self {
    Enemy {
      x,
      y,
      vx,
      vy: speed * 0.7,
      radius: ENEMY_M_RADIUS,
      etype: EnemyType::Medium,
      hp: 2,
      zigzag_phase: rng.float() * 6.2831853,
    }
  }

  fn new_heavy(rng: &mut Rng, x: f32, y: f32, vx: f32, speed: f32) -> Self {
    Enemy {
      x,
      y,
      vx,
      vy: speed * 0.45,
      radius: ENEMY_H_RADIUS,
      etype: EnemyType::Heavy,
      hp: 3,
      zigzag_phase: rng.float() * 6.2831853,
    }
  }

  fn new_small2(rng: &mut Rng, x: f32, y: f32, vx: f32, speed: f32) -> Self {
    let mut e = Enemy {
      x,
      y,
      vx,
      vy: speed * 1.3,
      radius: ENEMY_S_RADIUS,
      etype: EnemyType::Small2,
      hp: 1,
      zigzag_phase: rng.float() * 6.2831853,
    };
    e.vx *= 0.0; // zigzag handled in update
    e
  }

  fn color(&self) -> Rgb565 {
    match self.etype {
      EnemyType::Small => CLR_ENEMY_S,
      EnemyType::Medium => CLR_ENEMY_M,
      EnemyType::Heavy => CLR_ENEMY_H,
      EnemyType::Small2 => CLR_ENEMY_S2,
    }
  }

  fn update(&mut self, dt: f32, t: f32) {
    // Zigzag for Small2 type
    if self.etype == EnemyType::Small2 {
      self.x += lib::trig::fast_cos(self.zigzag_phase + t * 4.0) * 22.0 * dt;
    }
    self.x += self.vx * dt;
    self.y += self.vy * dt;
  }

  fn draw(&self, canvas: &mut Canvas) {
    let cx = self.x as i32;
    let cy = self.y as i32;
    let r = self.radius as i32;
    let inner = self.color();

    // Main body
    canvas.fill_circle(Point::new(cx, cy), r, inner);
    canvas.draw_circle(Point::new(cx, cy), r, Rgb565::BLACK);

    // Hit indicators — draw segments
    let seg_angle = 2.0943951; // 120 degrees
    for i in 0..self.hp {
      let a = seg_angle * i as f32;
      let ix = (cx as f32 + lib::trig::fast_cos(a + 1.5707963) * (r as f32 * 0.55)) as i32;
      let iy = (cy as f32 + lib::trig::fast_sin(a + 1.5707963) * (r as f32 * 0.55)) as i32;
      canvas.fill_circle(Point::new(ix, iy), 2, Rgb565::WHITE);
    }
  }
}

// ── Starfield ───────────────────────────────────────────────────────
fn init_stars(rng: &mut Rng) -> [(i32, i32, u8); NUM_STARS] {
  let mut stars = [(0i32, 0i32, 0u8); NUM_STARS];
  for i in 0..NUM_STARS {
    stars[i] = (rng.range(SCREEN_WIDTH) as i32, rng.range(SCREEN_HEIGHT) as i32, rng.range(3) as u8);
  }
  stars
}

fn draw_stars(canvas: &mut Canvas, stars: &[(i32, i32, u8); NUM_STARS]) {
  for &(x, y, layer) in stars {
    let color = match layer {
      0 => Rgb565::GRAY,
      1 => Rgb565::LIGHT_GRAY,
      _ => Rgb565::WHITE,
    };
    canvas.set_pixel(x, y, color);
  }
}

// ── Game state ──────────────────────────────────────────────────────
const S_MENU: u8 = 0;
const S_PLAY: u8 = 1;
const S_WAVE_CLEAR: u8 = 2;
const S_GAMEOVER: u8 = 3;

struct GameState {
  ship: Ship,
  bullets: Vec<Bullet>,
  enemies: Vec<Enemy>,
  stars: [(i32, i32, u8); NUM_STARS],
  rng: Rng,
  score: u32,
  lives: u32,
  level: u32,
  state: u8,
  spawn_timer: u32,
  enemies_to_spawn: u32,
  enemies_spawned: u32,
  wave_start: u32,
}

impl GameState {
  fn new() -> Self {
    let seed = get_millis();
    let mut rng = Rng::new(seed);
    let stars = init_stars(&mut rng);
    let gs = GameState {
      ship: Ship::new(),
      bullets: Vec::with_capacity(48),
      enemies: Vec::with_capacity(32),
      stars,
      rng,
      score: 0,
      lives: 3,
      level: 0,
      state: S_MENU,
      spawn_timer: 0,
      enemies_to_spawn: 0,
      enemies_spawned: 0,
      wave_start: seed,
    };
    gs
  }

  fn start_game(&mut self) {
    self.ship.reset();
    self.bullets.clear();
    self.enemies.clear();
    self.score = 0;
    self.lives = 3;
    self.level = 0;
    self.spawn_timer = 0;
    self.enemies_to_spawn = 8;
    self.enemies_spawned = 0;
    self.wave_start = get_millis();
    self.state = S_PLAY;
  }

  fn start_next_wave(&mut self) {
    self.level += 1;
    self.bullets.clear();
    // Clear enemies, but keep bullets from persisting between waves
    self.enemies.clear();
    self.ship.fire_cd = 0;
    self.spawn_timer = 0;
    let base = 8 + self.level as u32;
    let heavy = if self.level >= 3 { 1u32 } else { 0 };
    let medium = if self.level >= 2 { 2u32 } else { 0 };
    let small2 = if self.level >= 4 { 2u32 } else { 0 };
    let small = base.saturating_sub(heavy + medium + small2);
    self.enemies_to_spawn = small + medium + heavy + small2;
    self.enemies_spawned = 0;
    self.wave_start = get_millis();
    self.state = S_PLAY;
  }

  fn spawn_enemy(&mut self, level: u32) {
    let speed = 30.0 + level as f32 * 4.0;
    let x = self.rng.float() * (SW as f32 - 40.0) + 20.0;
    // Weighted random type
    let roll = self.rng.range(100);
    let enemy = if roll < 60 {
      // 60% small
      Enemy::new_small(&mut self.rng, x, -20.0, 0.0, speed)
    } else if roll < 80 {
      Enemy::new_medium(&mut self.rng, x, -20.0, 0.0, speed)
    } else if roll < 88 && level >= 3 {
      Enemy::new_heavy(&mut self.rng, x, -24.0, 0.0, speed)
    } else if roll < 96 && level >= 4 {
      Enemy::new_small2(&mut self.rng, x, -20.0, 0.0, speed)
    } else {
      // Fallback small
      Enemy::new_small(&mut self.rng, x, -20.0, 0.0, speed)
    };
    self.enemies.push(enemy);
  }

  fn draw_hud(&self, canvas: &mut Canvas) {
    let mut buf = [0u8; 48];
    let mut len: usize;

    len = 0;
    append_str(&mut buf, &mut len, "SCORE ");
    append_u32(&mut buf, &mut len, self.score);
    let text = core::str::from_utf8(&buf[..len]).unwrap();
    canvas.draw_text(text, 5, 5, CLR_HUD, 1);

    len = 0;
    append_str(&mut buf, &mut len, "LIVES ");
    append_u32(&mut buf, &mut len, self.lives);
    let text = core::str::from_utf8(&buf[..len]).unwrap();
    canvas.draw_text(text, SW as i32 - 50, 5, CLR_HUD, 1);

    len = 0;
    append_str(&mut buf, &mut len, "LVL ");
    append_u32(&mut buf, &mut len, self.level);
    let text = core::str::from_utf8(&buf[..len]).unwrap();
    canvas.draw_text(text, (SW as i32 - 32) / 2, 5, CLR_HUD, 1);
  }
}

// ── Collision helpers ───────────────────────────────────────────────
fn dist_sq(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
  let dx = ax - bx;
  let dy = ay - by;
  dx * dx + dy * dy
}

fn rect_circle_overlap(rx: f32, ry: f32, rw: f32, rh: f32, cx: f32, cy: f32, cr: f32) -> bool {
  let closest_x = cx.max(rx).min(rx + rw);
  let closest_y = cy.max(ry).min(ry + rh);
  let dx = cx - closest_x;
  let dy = cy - closest_y;
  dx * dx + dy * dy <= cr * cr
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

    print_line("Shooter starting\n");

    let mut state = GameState::new();
    let mut subscriber = log_error!(HOST_IPC_CHANNEL.subscriber(), "shooter: subscriber");
    let mut last_frame = get_millis();
    let mut ship_left = false;
    let mut ship_right = false;
    let mut ship_up = false;
    let mut ship_down = false;

    loop {
      let now = get_millis();
      let delta = now.wrapping_sub(last_frame);
      last_frame = now;
      let delta = if delta > 50 { 50 } else { delta };
      let dt = delta as f32 * 0.001;
      let t = now as f32 * 0.001;

      // ── Input ────────────────────────────────────────────────────
      loop {
        if let Some((_, msg)) = subscriber.try_next_message_pure() {
          if let HostIpcMessage::HexButton(btn) = msg {
            match btn {
              HexButton::Left => ship_left = true,
              HexButton::LeftReleased => ship_left = false,
              HexButton::Right => ship_right = true,
              HexButton::RightReleased => ship_right = false,
              HexButton::Up => ship_up = true,
              HexButton::UpReleased => ship_up = false,
              HexButton::Down => ship_down = true,
              HexButton::DownReleased => ship_down = false,
              HexButton::Fire => {
                if state.state != S_PLAY {
                  if state.state == S_GAMEOVER || state.state == S_MENU {
                    state.start_game();
                  }
                } else {
                  state.ship.try_fire(&mut state.bullets);
                }
              }
              _ => {}
            }
          }
        } else {
          break;
        }
      }

      // ── Update ───────────────────────────────────────────────────
      match state.state {
        S_MENU => {
          // waiting for Fire to start
        }
        S_PLAY => {
          // Ship
          state.ship.update(delta, ship_left, ship_right, ship_up, ship_down);

          // Bullets — update, cull off-screen
          let mut i = 0;
          while i < state.bullets.len() {
            let b = &mut state.bullets[i];
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            b.ttl = b.ttl.saturating_sub(delta);
            if b.ttl == 0 || b.y < 0.0 || b.x < -5.0 || b.x > SW as f32 + 5.0 || b.y > SH as f32 + 5.0 {
              state.bullets.swap_remove(i);
              continue;
            }
            i += 1;
          }

          // Enemies — spawn
          if state.enemies_spawned < state.enemies_to_spawn {
            state.spawn_timer = state.spawn_timer.saturating_add(delta);
            let interval = if state.level == 0 { 350 } else { (350 - state.level * 30).max(150) };
            if state.spawn_timer >= interval {
              state.spawn_timer = 0;
              state.spawn_enemy(state.level);
              state.enemies_spawned += 1;
            }
          }

          // Enemies — update
          for e in &mut state.enemies {
            e.update(dt, t);
          }

          // Cull enemies that went off bottom (they "escaped")
          state.enemies.retain(|e| e.y < SH as f32 + 30.0);

          // Bullet-enemy collision
          let mut bi = 0;
          while bi < state.bullets.len() {
            let mut hit = None;
            for (ei, e) in state.enemies.iter().enumerate() {
              if dist_sq(state.bullets[bi].x, state.bullets[bi].y, e.x, e.y) < (e.radius + 2.0) * (e.radius + 2.0) {
                hit = Some(ei);
                break;
              }
            }
            if let Some(ei) = hit {
              let e = &mut state.enemies[ei];
              e.hp -= 1;
              if e.hp == 0 {
                // Score by type
                let pts = match e.etype {
                  EnemyType::Small | EnemyType::Small2 => 50,
                  EnemyType::Medium => 100,
                  EnemyType::Heavy => 200,
                };
                state.score += pts * (1 + state.level);
                state.enemies.swap_remove(ei);
              }
              state.bullets.swap_remove(bi);
            } else {
              bi += 1;
            }
          }

          // Ship-enemy collision
          let mut hit = false;
          let ship_rect_x = state.ship.x - SHIP_W / 2.0;
          let ship_rect_y = state.ship.y - SHIP_H / 2.0;
          for e in &state.enemies {
            if rect_circle_overlap(ship_rect_x, ship_rect_y, SHIP_W, SHIP_H, e.x, e.y, e.radius) {
              hit = true;
              break;
            }
          }
          if hit {
            state.lives -= 1;
            state.bullets.clear();
            if state.lives > 0 {
              state.ship.reset();
              state.enemies.clear();
              state.spawn_timer = 0;
              state.enemies_to_spawn = (state.enemies_to_spawn / 2).max(4);
              state.enemies_spawned = 0;
            } else {
              state.state = S_GAMEOVER;
            }
          }

          // Wave clear
          if state.enemies_spawned >= state.enemies_to_spawn && state.enemies.is_empty() {
            state.state = S_WAVE_CLEAR;
          }
        }
        S_WAVE_CLEAR => {
          // Brief pause before next wave
          if now.wrapping_sub(state.wave_start) >= 1200 {
            state.start_next_wave();
          }
        }
        S_GAMEOVER => {}
        _ => {}
      }

      // ── Render ───────────────────────────────────────────────────
      canvas.clear(CLR_BG);
      draw_stars(&mut canvas, &state.stars);

      // Safe zone (green gradient bar at bottom)
      canvas.fill_rect(Rect::new(0, SH as i32 - 30, SW, 6), Rgb565::from_rgb888(20, 140, 50));

      if state.state == S_MENU {
        // Title screen
        canvas.draw_text("SHOOTER", (SW - 7 * 6 * 3) / 2, 60, Rgb565::WHITE, 3);
        canvas.draw_text("LEFT/RIGHT MOVE", 44, 100, Rgb565::LIGHT_GRAY, 1);
        canvas.draw_text("UP/DOWN NUDGE", 60, 112, Rgb565::LIGHT_GRAY, 1);
        canvas.draw_text("FIRE SHOOT", 80, 124, Rgb565::LIGHT_GRAY, 1);
        canvas.draw_text("PRESS FIRE TO START", 36, 180, Rgb565::YELLOW, 1);
      } else {
        // Game HUD
        state.draw_hud(&mut canvas);

        // Enemies
        for e in &state.enemies {
          e.draw(&mut canvas);
        }

        // Bullets
        for b in &state.bullets {
          canvas.fill_rect(
            Rect::new((b.x - b.w / 2.0) as i32, (b.y - b.h / 2.0) as i32, b.w as i32, b.h as i32),
            CLR_BULLET,
          );
        }

        // Ship
        state.ship.draw(&mut canvas);

        if state.state == S_WAVE_CLEAR {
          let elapsed = now.wrapping_sub(state.wave_start);
          if elapsed < 1200 {
            canvas.draw_text("WAVE CLEARED!", (SW - 13 * 6 * 2) / 2, 80, Rgb565::YELLOW, 2);
            let mut buf = [0u8; 32];
            let mut len = 0;
            append_str(&mut buf, &mut len, "NEXT WAVE IN ");
            let rem = 1200u32 - elapsed;
            append_u32(&mut buf, &mut len, rem / 1000);
            let text = core::str::from_utf8(&buf[..len]).unwrap();
            canvas.draw_text(text, (SW - 16 * 6) / 2, 100, Rgb565::WHITE, 1);
          }
        }

        if state.state == S_GAMEOVER {
          canvas.draw_text("GAME OVER", (SW - 9 * 6 * 3) / 2, 70, Rgb565::RED, 3);
          let mut buf = [0u8; 32];
          let mut len = 0;
          append_str(&mut buf, &mut len, "SCORE ");
          append_u32(&mut buf, &mut len, state.score);
          let text = core::str::from_utf8(&buf[..len]).unwrap();
          canvas.draw_text(text, (SW - 10 * 6) / 2, 105, Rgb565::WHITE, 1);
          canvas.draw_text("PRESS FIRE", (SW - 9 * 6) / 2, 170, Rgb565::YELLOW, 1);
        }
      }

      unsafe {
        extern_set_lcd_buffer(canvas.as_ptr());
      }

      yield_now().await;
    }
  })());
}
