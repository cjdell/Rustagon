// Written by Local-LLM running on AMD Vega 8 iGPU
// A Tetris game for the badge.
//
// 10x20 grid, 10px cells, centered on the 240x240 screen.
// Controls: Left/Right to move, Up to rotate, Down for soft-drop, Fire for hard-drop.

#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use crate::lib::{
  gfx::{Canvas, Point, Rect, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::print_line,
  protocol::{HexButton, HostIpcMessage, extern_set_lcd_buffer},
  tasks::{HOST_IPC_CHANNEL, spawn, yield_now},
};
use alloc::boxed::Box;

// --- Board geometry ---
const BOARD_W: usize = 10;
const BOARD_H: usize = 20;
const CELL_SIZE: i32 = 10;
const BOARD_PIX_W: i32 = (BOARD_W as i32) * CELL_SIZE;
const BOARD_PIX_H: i32 = (BOARD_H as i32) * CELL_SIZE;
const BOARD_X: i32 = (SCREEN_WIDTH as i32 - BOARD_PIX_W) / 2;
const BOARD_Y: i32 = (SCREEN_HEIGHT as i32 - BOARD_PIX_H) / 2;

// --- Piece shapes (4x4 matrices, 1 = filled) ---
type Shape = [[u8; 4]; 4];

const SHAPE_LINE: Shape = [[0, 0, 0, 0], [1, 1, 1, 1], [0, 0, 0, 0], [0, 0, 0, 0]];
const SHAPE_SQUARE: Shape = [[0, 0, 0, 0], [0, 1, 1, 0], [0, 1, 1, 0], [0, 0, 0, 0]];
const SHAPE_T: Shape = [[0, 0, 0, 0], [1, 1, 1, 0], [0, 1, 0, 0], [0, 0, 0, 0]];
const SHAPE_L: Shape = [[0, 0, 0, 0], [1, 1, 1, 0], [1, 0, 0, 0], [0, 0, 0, 0]];
const SHAPE_J: Shape = [[0, 0, 0, 0], [1, 1, 1, 0], [0, 0, 1, 0], [0, 0, 0, 0]];
const SHAPE_S: Shape = [[0, 0, 0, 0], [0, 1, 1, 0], [1, 1, 0, 0], [0, 0, 0, 0]];
const SHAPE_Z: Shape = [[0, 0, 0, 0], [1, 1, 0, 0], [0, 1, 1, 0], [0, 0, 0, 0]];

const PIECES: [(Shape, u8); 7] = [
  (SHAPE_LINE, 1),
  (SHAPE_SQUARE, 2),
  (SHAPE_T, 3),
  (SHAPE_L, 4),
  (SHAPE_J, 5),
  (SHAPE_S, 6),
  (SHAPE_Z, 7),
];

// Color palette (index 0 = empty / black)
const PALETTE: [Rgb565; 8] = [
  Rgb565::BLACK,   // 0: empty
  Rgb565::CYAN,    // 1: LINE
  Rgb565::YELLOW,  // 2: SQUARE
  Rgb565::MAGENTA, // 3: T
  Rgb565::ORANGE,  // 4: L
  Rgb565::BLUE,    // 5: J
  Rgb565::GREEN,   // 6: S
  Rgb565::RED,     // 7: Z
];

// --- Random number generator (simple LCG) ---
struct Rng {
  state: u32,
}

impl Rng {
  fn next(&mut self) -> u32 {
    self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
    self.state
  }

  fn range(&mut self, max: usize) -> usize {
    (self.next() as usize) % max
  }
}

// --- Collision and placement helpers ---

/// Rotate a 4x4 shape 90 degrees clockwise in place.
fn rotate_cw(shape: &Shape) -> Shape {
  let mut result = [[0u8; 4]; 4];
  for r in 0..4 {
    for c in 0..4 {
      result[c][3 - r] = shape[r][c];
    }
  }
  result
}

/// Check if `shape` can be placed at `(px, py)` on the board without overlapping
/// existing pieces or going out of bounds.
fn can_place(board: &[[u8; BOARD_W]; BOARD_H], shape: &Shape, px: i32, py: i32) -> bool {
  for r in 0..4 {
    for c in 0..4 {
      if shape[r][c] == 0 {
        continue;
      }
      let bx = px + c as i32;
      let by = py + r as i32;
      if bx < 0 || bx >= BOARD_W as i32 || by < 0 || by >= BOARD_H as i32 {
        return false;
      }
      if board[by as usize][bx as usize] != 0 {
        return false;
      }
    }
  }
  true
}

/// Write the piece's cells into the board at `(px, py)` using `color` as the palette index.
fn place_piece(board: &mut [[u8; BOARD_W]; BOARD_H], shape: &Shape, px: i32, py: i32, color: u8) {
  for r in 0..4 {
    for c in 0..4 {
      if shape[r][c] == 0 {
        continue;
      }
      let bx = px + c as i32;
      let by = py + r as i32;
      if bx >= 0 && bx < BOARD_W as i32 && by >= 0 && by < BOARD_H as i32 {
        board[by as usize][bx as usize] = color;
      }
    }
  }
}

/// Remove full lines, shifting everything above down. Returns the number of lines cleared.
fn clear_lines(board: &mut [[u8; BOARD_W]; BOARD_H]) -> usize {
  let mut lines_cleared = 0;
  let mut y: i32 = (BOARD_H - 1) as i32;
  while y >= 0 {
    let mut full = true;
    for x in 0..BOARD_W {
      if board[y as usize][x] == 0 {
        full = false;
        break;
      }
    }
    if full {
      for yy in (1..=y).rev() {
        board[yy as usize] = board[(yy - 1) as usize];
      }
      board[0] = [0u8; BOARD_W];
      lines_cleared += 1;
      // Stay at the same y — the line above has shifted down
    } else {
      y -= 1;
    }
  }
  lines_cleared
}

// --- Game state ---

struct GameState {
  board: [[u8; BOARD_W]; BOARD_H],
  rng: Rng,
  piece_shape: Shape,
  piece_x: i32,
  piece_y: i32,
  piece_color: u8,
  next_shape: Shape,
  next_color: u8,
  last_drop: u32,
  drop_interval_ms: u32,
  score: u32,
  lines: u32,
  level: u32,
  game_over: bool,
}

impl GameState {
  fn new() -> Self {
    let seed = crate::lib::helper::get_millis();
    let mut rng = Rng { state: seed };
    let (shape, color) = Self::random_piece(&mut rng);
    let (next_shape, next_color) = Self::random_piece(&mut rng);
    let mut state = Self {
      board: [[0u8; BOARD_W]; BOARD_H],
      rng,
      piece_shape: shape,
      piece_x: 3,
      piece_y: 0,
      piece_color: color,
      next_shape,
      next_color,
      last_drop: seed,
      drop_interval_ms: 800,
      score: 0,
      lines: 0,
      level: 0,
      game_over: false,
    };
    state.spawn_piece();
    state
  }

  fn random_piece(rng: &mut Rng) -> (Shape, u8) {
    let idx = rng.range(PIECES.len());
    (PIECES[idx].0, PIECES[idx].1)
  }

  fn spawn_piece(&mut self) {
    self.piece_shape = self.next_shape;
    self.piece_color = self.next_color;
    self.piece_x = 3;
    self.piece_y = 0;
    let (shape, color) = Self::random_piece(&mut self.rng);
    self.next_shape = shape;
    self.next_color = color;
    if !can_place(&self.board, &self.piece_shape, self.piece_x, self.piece_y) {
      self.game_over = true;
    }
  }

  fn try_move(&mut self, dx: i32, dy: i32) -> bool {
    let new_x = self.piece_x + dx;
    let new_y = self.piece_y + dy;
    if can_place(&self.board, &self.piece_shape, new_x, new_y) {
      self.piece_x = new_x;
      self.piece_y = new_y;
      true
    } else {
      false
    }
  }

  fn try_rotate(&mut self) {
    let rotated = rotate_cw(&self.piece_shape);
    if can_place(&self.board, &rotated, self.piece_x, self.piece_y) {
      self.piece_shape = rotated;
    }
  }

  fn hard_drop(&mut self) {
    while can_place(&self.board, &self.piece_shape, self.piece_x, self.piece_y + 1) {
      self.piece_y += 1;
    }
  }

  fn lock_piece(&mut self) {
    if self.game_over {
      return;
    }
    place_piece(&mut self.board, &self.piece_shape, self.piece_x, self.piece_y, self.piece_color);
    let cleared = clear_lines(&mut self.board);
    if cleared > 0 {
      self.lines += cleared as u32;
      let points = match cleared {
        1 => 100,
        2 => 300,
        3 => 500,
        _ => 800,
      };
      self.score += points * (self.level + 1);
      self.level = self.lines / 10;
      let new_interval = 800 - self.level * 50;
      self.drop_interval_ms = if new_interval < 100 { 100 } else { new_interval };
    }
    self.spawn_piece();
  }

  fn update(&mut self, now: u32) {
    if self.game_over {
      return;
    }
    if now.wrapping_sub(self.last_drop) >= self.drop_interval_ms {
      self.last_drop = now;
      if !self.try_move(0, 1) {
        self.lock_piece();
      }
    }
  }

  /// Draw a 4x4 piece shape into the canvas at grid-relative offset (gx, gy),
  /// with cells positioned at base_x + (gx + c) * CELL_SIZE.
  fn draw_shape(canvas: &mut Canvas, shape: &Shape, color_idx: u8, gx: i32, gy: i32, base_x: i32, base_y: i32) {
    for r in 0..4 {
      for c in 0..4 {
        if shape[r][c] != 0 {
          let px = base_x + (gx + c as i32) * CELL_SIZE;
          let py = base_y + (gy + r as i32) * CELL_SIZE;
          canvas.fill_rect(Rect::new(px, py, CELL_SIZE, CELL_SIZE), PALETTE[color_idx as usize]);
        }
      }
    }
  }

  fn draw(&self, canvas: &mut Canvas) {
    // Board background
    canvas.fill_rect(Rect::new(BOARD_X, BOARD_Y, BOARD_PIX_W, BOARD_PIX_H), Rgb565::DARK_GRAY);

    // Placed pieces
    for y in 0..BOARD_H {
      for x in 0..BOARD_W {
        if self.board[y][x] != 0 {
          let px = BOARD_X + (x as i32) * CELL_SIZE;
          let py = BOARD_Y + (y as i32) * CELL_SIZE;
          canvas.fill_rect(Rect::new(px, py, CELL_SIZE, CELL_SIZE), PALETTE[self.board[y][x] as usize]);
        }
      }
    }

    // Current piece
    if !self.game_over {
      Self::draw_shape(
        canvas,
        &self.piece_shape,
        self.piece_color,
        self.piece_x,
        self.piece_y,
        BOARD_X,
        BOARD_Y,
      );
    }

    // Grid lines
    let grid_color = Rgb565::BLACK;
    for i in 0..=BOARD_W {
      let x = BOARD_X + (i as i32) * CELL_SIZE;
      canvas.draw_line(Point::new(x, BOARD_Y), Point::new(x, BOARD_Y + BOARD_PIX_H), grid_color);
    }
    for i in 0..=BOARD_H {
      let y = BOARD_Y + (i as i32) * CELL_SIZE;
      canvas.draw_line(Point::new(BOARD_X, y), Point::new(BOARD_X + BOARD_PIX_W, y), grid_color);
    }

    // UI — score, lines, next piece preview
    let ui_x = BOARD_X + BOARD_PIX_W + 10;

    // Score
    let mut buf = [0u8; 32];
    let mut len = 0;
    crate::lib::fmt::append_str(&mut buf, &mut len, "SCORE ");
    crate::lib::fmt::append_u32(&mut buf, &mut len, self.score);
    let score_text = core::str::from_utf8(&buf[..len]).unwrap();
    canvas.draw_text(score_text, ui_x, BOARD_Y + 10, Rgb565::WHITE, 1);

    // Lines
    len = 0;
    crate::lib::fmt::append_str(&mut buf, &mut len, "LINES ");
    crate::lib::fmt::append_u32(&mut buf, &mut len, self.lines);
    let lines_text = core::str::from_utf8(&buf[..len]).unwrap();
    canvas.draw_text(lines_text, ui_x, BOARD_Y + 25, Rgb565::WHITE, 1);

    // Next piece label
    canvas.draw_text("NEXT", ui_x, BOARD_Y + 50, Rgb565::WHITE, 1);

    // Next piece preview
    Self::draw_shape(canvas, &self.next_shape, self.next_color, 0, 0, ui_x + 15, BOARD_Y + 65);
  }
}

// --- Main ---

#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async move || {
    let mut buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);
    let mut canvas = Canvas::new(&mut buf[..], SCREEN_WIDTH, SCREEN_HEIGHT);

    print_line("Tetris starting\n");

    let mut state = GameState::new();
    let mut subscriber = log_error!(HOST_IPC_CHANNEL.subscriber(), "tetris: subscriber");

    let mut game_over_start: u32 = 0;

    loop {
      let now = crate::lib::helper::get_millis();

      // Process input
      if !state.game_over {
        if let Some((_, msg)) = subscriber.try_next_message_pure() {
          match msg {
            HostIpcMessage::HexButton(HexButton::Left) => {
              state.try_move(-1, 0);
            }
            HostIpcMessage::HexButton(HexButton::Right) => {
              state.try_move(1, 0);
            }
            HostIpcMessage::HexButton(HexButton::Up) => {
              state.try_rotate();
            }
            HostIpcMessage::HexButton(HexButton::Down) => {
              if !state.try_move(0, 1) {
                state.lock_piece();
              }
            }
            HostIpcMessage::HexButton(HexButton::Fire) => {
              state.hard_drop();
              state.lock_piece();
            }
            _ => {}
          }
        }

        // Update
        state.update(now);
      }

      // Track game over time
      if state.game_over && game_over_start == 0 {
        game_over_start = now;
      }

      // Render
      canvas.clear(Rgb565::BLACK);

      // Title
      canvas.draw_text("TETRIS", (SCREEN_WIDTH as i32 - 72) / 2, 5, Rgb565::WHITE, 2);

      state.draw(&mut canvas);

      if state.game_over {
        let elapsed = now.wrapping_sub(game_over_start);
        if elapsed >= 2000 {
          break;
        }
        // "GAME OVER" centered (9 chars * 6 * 3 = 162 px wide)
        canvas.draw_text("GAME OVER", (SCREEN_WIDTH as i32 - 162) / 2, 108, Rgb565::RED, 3);
      }

      unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

      yield_now().await;
    }

    print_line("Tetris done\n");
  })());
}
