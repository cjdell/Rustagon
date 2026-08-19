// A Snake game rendered with the zero-dependency gfx library.
//
// A simple snake game where the player controls the direction of a snake
// that grows by eating food. Controls: Left/Right/Up/Down arrows, Fire to
// restart on game over.

#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use crate::lib::{
  gfx::{Canvas, Point, Rect, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::get_millis,
  protocol::{extern_set_lcd_buffer, HexButton, HostIpcMessage},
  tasks::{spawn, yield_now, HOST_IPC_CHANNEL},
};
use alloc::{boxed::Box, vec, vec::Vec};

const GRID_W: usize = 20;
const GRID_H: usize = 20;
const CELL: i32 = 12;
const GRID_W_PIX: i32 = (GRID_W as i32) * CELL;
const GRID_H_PIX: i32 = (GRID_H as i32) * CELL;
const GRID_X: i32 = (SCREEN_WIDTH as i32 - GRID_W_PIX) / 2;
const GRID_Y: i32 = (SCREEN_HEIGHT as i32 - GRID_H_PIX) / 2;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Dir {
  x: i32,
  y: i32,
}

impl Dir {
  fn new(x: i32, y: i32) -> Self {
    Dir { x, y }
  }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
  x: i32,
  y: i32,
}

#[derive(Clone)]
struct Snake {
  body: Vec<Cell>,
  dir: Dir,
}

impl Snake {
  fn new() -> Self {
    Snake {
      body: vec![Cell {
        x: 10 * CELL,
        y: 10 * CELL,
      }],
      dir: Dir::new(1, 0),
    }
  }
}

#[derive(Clone)]
struct Game {
  snake: Snake,
  food: Cell,
  score: u32,
  over: bool,
}

impl Game {
  fn new() -> Self {
    Game {
      snake: Snake::new(),
      food: Cell { x: 0, y: 0 },
      score: 0,
      over: false,
    }
  }

  fn place_food(&mut self) {
    for y in 0..GRID_H {
      for x in 0..GRID_W {
        let cx = x as i32 * CELL;
        let cy = y as i32 * CELL;
        if !self.snake.body.iter().any(|c| c.x == cx && c.y == cy) {
          self.food = Cell { x: cx, y: cy };
          return;
        }
      }
    }
  }

  fn step(&mut self) -> bool {
    if self.over {
      return false;
    }
    let head = self.snake.body[0];
    let new_head = Cell {
      x: head.x + self.snake.dir.x,
      y: head.y + self.snake.dir.y,
    };
    if self.collides(&new_head) {
      self.over = true;
      return false;
    }
    self.snake.body.insert(0, new_head);
    if new_head == self.food {
      self.score += 1;
      self.place_food();
    } else {
      self.snake.body.pop();
    }
    true
  }

  fn collides(&self, pos: &Cell) -> bool {
    if pos.x < 0 || pos.y < 0 || pos.x >= GRID_W_PIX || pos.y >= GRID_H_PIX {
      return true;
    }
    self.snake.body.iter().any(|c| c.x == pos.x && c.y == pos.y)
  }

  fn reset(&mut self) {
    self.snake = Snake::new();
    self.food = Cell { x: 0, y: 0 };
    self.score = 0;
    self.over = false;
    self.place_food();
  }
}

fn handle_input(game: &mut Game, btn: HexButton) {
  match btn {
    HexButton::Left => {
      if game.snake.dir.x + (-1) != 0 && game.snake.dir.y + 0 != 0 {
        game.snake.dir = Dir::new(-1, 0);
      }
    }
    HexButton::Right => {
      if game.snake.dir.x + 1 != 0 && game.snake.dir.y + 0 != 0 {
        game.snake.dir = Dir::new(1, 0);
      }
    }
    HexButton::Up => {
      if game.snake.dir.x + 0 != 0 && game.snake.dir.y + (-1) != 0 {
        game.snake.dir = Dir::new(0, -1);
      }
    }
    HexButton::Down => {
      if game.snake.dir.x + 0 != 0 && game.snake.dir.y + 1 != 0 {
        game.snake.dir = Dir::new(0, 1);
      }
    }
    HexButton::Fire => {
      game.reset();
    }
    _ => {}
  }
}

fn draw_grid(canvas: &mut Canvas) {
  canvas.fill_rect(Rect::new(GRID_X, GRID_Y, GRID_W_PIX, GRID_H_PIX), Rgb565::DARK_GRAY);
  let border = Rgb565::GRAY;
  for x in 0..=GRID_W {
    let px = GRID_X + x as i32 * CELL;
    canvas.draw_line(Point::new(px, GRID_Y), Point::new(px, GRID_Y + GRID_H_PIX), border);
  }
  for y in 0..=GRID_H {
    let py = GRID_Y + y as i32 * CELL;
    canvas.draw_line(Point::new(GRID_X, py), Point::new(GRID_X + GRID_W_PIX, py), border);
  }
}

fn draw_cell(canvas: &mut Canvas, x: i32, y: i32, color: Rgb565) {
  let px = GRID_X + x;
  let py = GRID_Y + y;
  canvas.fill_rect(Rect::new(px, py, CELL, CELL), color);
}

fn render(game: &Game, canvas: &mut Canvas) {
  canvas.clear(Rgb565::BLACK);
  draw_grid(canvas);

  // Food
  draw_cell(canvas, game.food.x, game.food.y, Rgb565::RED);

  // Snake
  for (i, cell) in game.snake.body.iter().enumerate() {
    let color = if i == 0 {
      Rgb565::GREEN
    } else if i == 1 {
      Rgb565::YELLOW
    } else {
      Rgb565::GREEN
    };
    draw_cell(canvas, cell.x, cell.y, color);
  }

  // Score
  let mut buf = [0u8; 32];
  let mut len = 0;
  lib::fmt::append_str(&mut buf, &mut len, "SCORE: ");
  lib::fmt::append_u32(&mut buf, &mut len, game.score);
  if let Ok(text) = core::str::from_utf8(&buf[..len]) {
    canvas.draw_text(text, GRID_X + GRID_W_PIX + 8, GRID_Y + 4, Rgb565::WHITE, 1);
  }

  // Game over
  if game.over {
    canvas.draw_text("GAME OVER", (SCREEN_WIDTH as i32 - 162) / 2, 80, Rgb565::RED, 2);
    canvas.draw_text("Fire to restart", (SCREEN_WIDTH as i32 - 162) / 2, 100, Rgb565::WHITE, 1);
  }
}

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
    let mut subscriber = log_error!(HOST_IPC_CHANNEL.subscriber(), "snake: subscriber");
    let mut start = get_millis();

    loop {
      let now = get_millis();

      if let Some((_, msg)) = subscriber.try_next_message_pure() {
        if let HostIpcMessage::HexButton(btn) = msg {
          handle_input(&mut game, btn);
        }
      }

      // Update every 150ms (~6.7 FPS)
      if now.wrapping_sub(start) >= 150 {
        start = now;
        game.step();
      }

      render(&game, &mut canvas);
      unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

      yield_now().await;
    }
  })());
}
