// A non-blocking version of the cube demo (yield)

#![no_std]
#![no_main]

use sdk as lib;

extern crate alloc;

use crate::lib::{
  gfx::{Canvas, Point, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::get_millis,
  protocol::extern_set_lcd_buffer,
  tasks::{spawn, yield_now},
  trig::{fast_cos, fast_sin},
};
use alloc::boxed::Box;
use core::f32::consts::PI;

// Define a 3D point
#[derive(Clone, Copy)]
struct Point3D {
  x: f32,
  y: f32,
  z: f32,
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
  let cos_a = fast_cos(angle);
  let sin_a = fast_sin(angle);
  Point3D {
    x: point.x,
    y: point.y * cos_a - point.z * sin_a,
    z: point.y * sin_a + point.z * cos_a,
  }
}

// Rotate a 3D point around the Y axis
fn rotate_y(point: Point3D, angle: f32) -> Point3D {
  let cos_a = fast_cos(angle);
  let sin_a = fast_sin(angle);
  Point3D {
    x: point.x * cos_a + point.z * sin_a,
    y: point.y,
    z: -point.x * sin_a + point.z * cos_a,
  }
}

// Rotate a 3D point around the Z axis
fn rotate_z(point: Point3D, angle: f32) -> Point3D {
  let cos_a = fast_cos(angle);
  let sin_a = fast_sin(angle);
  Point3D {
    x: point.x * cos_a - point.y * sin_a,
    y: point.x * sin_a + point.y * cos_a,
    z: point.z,
  }
}

// Project a 3D point to 2D with perspective
fn project_3d_to_2d(point: Point3D, distance: f32) -> Point {
  let scale = distance / (distance + point.z);
  Point::new(CUBE_CENTER_X + (point.x * scale) as i32, CUBE_CENTER_Y + (point.y * scale) as i32)
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
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let mut buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);
    let mut canvas = Canvas::new(&mut buf[..], SCREEN_WIDTH, SCREEN_HEIGHT);

    let start = get_millis();

    // Initialize cube points
    let cube_points = create_cube_points();

    loop {
      let now = get_millis();
      let elapsed = now - start;

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
      let projected_points: [Point; 8] = rotated_points.map(|p| project_3d_to_2d(p, 200.0));

      // Clear display
      canvas.clear(Rgb565::BLACK);

      // Draw all edges of the cube
      for &(start_idx, end_idx) in CUBE_EDGES.iter() {
        canvas.draw_line(projected_points[start_idx], projected_points[end_idx], Rgb565::WHITE);
      }

      // Update the display
      unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

      // Yield to allow other tasks to run
      yield_now().await;
    }
  })());
}
