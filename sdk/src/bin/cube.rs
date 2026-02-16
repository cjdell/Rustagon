// A non-blocking version of the cube demo (yield)

#![no_std]
#![no_main]
#![feature(future_join)]
#![feature(thread_local)]

#[path = "../lib/mod.rs"]
#[macro_use]
mod lib;

extern crate alloc;

use crate::lib::{
  graphics::{BufferTarget, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::get_millis,
  protocol::extern_set_lcd_buffer,
  tasks::{spawn, yield_now},
};
use alloc::boxed::Box;
use core::f32::consts::PI;
use embedded_graphics::{
  Drawable as _,
  pixelcolor::Rgb565,
  prelude::{Point, Primitive as _, RgbColor},
  primitives::{Line, PrimitiveStyle},
};

// Define a 3D point
#[derive(Clone, Copy)]
struct Point3D {
  x: f32,
  y: f32,
  z: f32,
}

// Define a 2D point for display
#[derive(Clone, Copy)]
struct Point2D {
  x: i32,
  y: i32,
}

impl Point2D {
  fn new(x: i32, y: i32) -> Self {
    Point2D { x, y }
  }
}

// Define a cube with 8 vertices
const CUBE_SIZE: f32 = 40.0;
const CUBE_CENTER_X: i32 = SCREEN_WIDTH as i32 / 2;
const CUBE_CENTER_Y: i32 = SCREEN_HEIGHT as i32 / 2;

// Define the 8 corners of the cube
fn create_cube_points() -> [Point3D; 8] {
  [
    Point3D {
      x: -CUBE_SIZE,
      y: -CUBE_SIZE,
      z: -CUBE_SIZE,
    },
    Point3D {
      x: CUBE_SIZE,
      y: -CUBE_SIZE,
      z: -CUBE_SIZE,
    },
    Point3D {
      x: CUBE_SIZE,
      y: CUBE_SIZE,
      z: -CUBE_SIZE,
    },
    Point3D {
      x: -CUBE_SIZE,
      y: CUBE_SIZE,
      z: -CUBE_SIZE,
    },
    Point3D {
      x: -CUBE_SIZE,
      y: -CUBE_SIZE,
      z: CUBE_SIZE,
    },
    Point3D {
      x: CUBE_SIZE,
      y: -CUBE_SIZE,
      z: CUBE_SIZE,
    },
    Point3D {
      x: CUBE_SIZE,
      y: CUBE_SIZE,
      z: CUBE_SIZE,
    },
    Point3D {
      x: -CUBE_SIZE,
      y: CUBE_SIZE,
      z: CUBE_SIZE,
    },
  ]
}

// Rotate a 3D point around the X axis
fn rotate_x(point: Point3D, angle: f32) -> Point3D {
  let cos_a = angle.cos();
  let sin_a = angle.sin();
  Point3D {
    x: point.x,
    y: point.y * cos_a - point.z * sin_a,
    z: point.y * sin_a + point.z * cos_a,
  }
}

// Rotate a 3D point around the Y axis
fn rotate_y(point: Point3D, angle: f32) -> Point3D {
  let cos_a = angle.cos();
  let sin_a = angle.sin();
  Point3D {
    x: point.x * cos_a + point.z * sin_a,
    y: point.y,
    z: -point.x * sin_a + point.z * cos_a,
  }
}

// Rotate a 3D point around the Z axis
fn rotate_z(point: Point3D, angle: f32) -> Point3D {
  let cos_a = angle.cos();
  let sin_a = angle.sin();
  Point3D {
    x: point.x * cos_a - point.y * sin_a,
    y: point.x * sin_a + point.y * cos_a,
    z: point.z,
  }
}

// Project a 3D point to 2D with perspective
fn project_3d_to_2d(point: Point3D, distance: f32) -> Point2D {
  let scale = distance / (distance + point.z);
  let x = (point.x * scale) as i32;
  let y = (point.y * scale) as i32;
  Point2D::new(CUBE_CENTER_X + x, CUBE_CENTER_Y + y)
}

// Define the edges of the cube (connections between vertices)
const CUBE_EDGES: [(usize, usize); 12] = [
  (0, 1),
  (1, 2),
  (2, 3),
  (3, 0), // bottom face
  (4, 5),
  (5, 6),
  (6, 7),
  (7, 4), // top face
  (0, 4),
  (1, 5),
  (2, 6),
  (3, 7), // vertical edges
];

static ANIMATION_DURATION: usize = 10_000; // 10 seconds for a full rotation

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);

    let mut display = BufferTarget::new(buf);

    let line_style = PrimitiveStyle::with_stroke(Rgb565::WHITE, 1);
    let start = get_millis();
    let mut last_tick = start;

    // Initialize cube points
    let cube_points = create_cube_points();

    loop {
      let now = get_millis();
      let delta = now - last_tick;
      let elapsed = now - start;

      last_tick = now;

      // Stop after animation duration
      if now - start > ANIMATION_DURATION as u32 {
        break;
      }

      // Calculate rotation angles based on elapsed time
      // Rotate around X, Y, and Z axes simultaneously for a more interesting effect
      let rotation_speed = 10.0 * PI / (ANIMATION_DURATION as f32); // Full rotation in 10 seconds
      let angle_x = (elapsed as f32) * rotation_speed * 0.5;
      let angle_y = (elapsed as f32) * rotation_speed * 0.7;
      let angle_z = (elapsed as f32) * rotation_speed * 0.3;

      // Apply rotations to each point
      let mut rotated_points = [Point3D { x: 0.0, y: 0.0, z: 0.0 }; 8];
      for i in 0..8 {
        let mut p = cube_points[i];
        p = rotate_x(p, angle_x);
        p = rotate_y(p, angle_y);
        p = rotate_z(p, angle_z);
        rotated_points[i] = p;
      }

      // Project to 2D
      let projected_points: [Point2D; 8] = rotated_points.map(|p| project_3d_to_2d(p, 200.0));

      // Clear display
      display.clear();

      // Draw all edges of the cube
      for &(start_idx, end_idx) in CUBE_EDGES.iter() {
        let start_point = projected_points[start_idx];
        let end_point = projected_points[end_idx];

        Line::new(
          Point::new(start_point.x, start_point.y),
          Point::new(end_point.x, end_point.y),
        )
        .into_styled(line_style)
        .draw(&mut display)
        .unwrap();
      }

      // Update the display
      unsafe { extern_set_lcd_buffer(display.get_buffer_ptr()) };

      // Yield to allow other tasks to run
      yield_now().await;
    }
  })());
}
