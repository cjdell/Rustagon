// A 3D cube rendered with the zero-dependency gfx library.
//
// Port of the embedded-3dgfx "3dcube" sample: a fixed camera at (0, 2, 3)
// looking at the origin, one rotating cube drawn with flat-shaded filled
// triangles and white edges. No embedded-graphics, embedded-3dgfx or nalgebra.

#![no_std]
#![no_main]

use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt::{print_str, print_u32},
  gfx::{Canvas, Point, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::{get_millis, print_line},
  protocol::extern_set_lcd_buffer,
  tasks::{spawn, yield_now},
  trig::{fast_cos, fast_sin, fast_sqrt},
};
use alloc::boxed::Box;

static ANIMATION_DURATION: usize = 5_000;

// Cube vertices (front face first) and the 12 triangle faces.
const VERTICES: [[f32; 3]; 8] = [
  [-1.0, -1.0, 1.0],
  [1.0, -1.0, 1.0],
  [1.0, 1.0, 1.0],
  [-1.0, 1.0, 1.0],
  [-1.0, -1.0, -1.0],
  [1.0, -1.0, -1.0],
  [1.0, 1.0, -1.0],
  [-1.0, 1.0, -1.0],
];

const FACES: [[usize; 3]; 12] = [
  [0, 1, 2],
  [0, 2, 3], // front
  [5, 4, 7],
  [5, 7, 6], // back
  [3, 2, 6],
  [3, 6, 7], // top
  [4, 5, 1],
  [4, 1, 0], // bottom
  [1, 5, 6],
  [1, 6, 2], // right
  [4, 0, 3],
  [4, 3, 7], // left
];

const EDGES: [(usize, usize); 12] = [
  (0, 1),
  (1, 2),
  (2, 3),
  (3, 0), // front face
  (4, 5),
  (5, 6),
  (6, 7),
  (7, 4), // back face
  (0, 4),
  (1, 5),
  (2, 6),
  (3, 7), // vertical edges
];

// Camera at (0, 2, 3) looking at the origin. Precomputed basis (scaled by
// 1/sqrt(13) and folded into FOCAL).
const FOCAL: f32 = 260.0;
const INV_SQRT13: f32 = 0.2773500981126146;

fn rotate_y(x: f32, y: f32, z: f32, a: f32) -> (f32, f32, f32) {
  let (c, s) = (fast_cos(a), fast_sin(a));
  (x * c + z * s, y, -x * s + z * c)
}

fn rotate_x(x: f32, y: f32, z: f32, a: f32) -> (f32, f32, f32) {
  let (c, s) = (fast_cos(a), fast_sin(a));
  (x, y * c - z * s, y * s + z * c)
}

fn rotate_z(x: f32, y: f32, z: f32, a: f32) -> (f32, f32, f32) {
  let (c, s) = (fast_cos(a), fast_sin(a));
  (x * c - y * s, x * s + y * c, z)
}

/// Camera-space coordinates for a world-space vertex (y = up, z = forward).
fn to_camera(v: [f32; 3]) -> [f32; 3] {
  let dy = ((v[1] - 2.0) * 3.0 - (v[2] - 3.0) * 2.0) * INV_SQRT13;
  let dz = (-(v[1] - 2.0) * 2.0 - (v[2] - 3.0) * 3.0) * INV_SQRT13;
  [v[0], dy, dz]
}

/// Project a camera-space vertex to the screen.
fn project(cam: [f32; 3]) -> Point {
  Point::new(
    (cam[0] * FOCAL / cam[2] + SCREEN_WIDTH as f32 / 2.0) as i32,
    (cam[1] * FOCAL / cam[2] + SCREEN_HEIGHT as f32 / 2.0) as i32,
  )
}

fn shade(base: Rgb565, intensity: f32) -> Rgb565 {
  Rgb565::new(
    (base.r5() as f32 * intensity) as u8,
    (base.g6() as f32 * intensity) as u8,
    (base.b5() as f32 * intensity) as u8,
  )
}

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
    let mut last_tick = start;

    // Light direction (world space, from top-right-front)
    let light = [0.5f32, -0.5, -1.0];
    let light_len = fast_sqrt(light[0] * light[0] + light[1] * light[1] + light[2] * light[2]);

    loop {
      let now = get_millis();
      let delta = now - last_tick;
      let elapsed = now - start;
      last_tick = now;

      // 5 second animation duration
      if now - start > ANIMATION_DURATION as u32 {
        break;
      }

      print_str("delta: ");
      print_u32(delta);
      print_line("\n");

      let t = elapsed as f32 / 1000.0;
      let (yaw, pitch, roll) = (t * 0.3, t, t * 0.5);

      // Rotate, transform to camera space and project.
      let mut cam = [[0f32; 3]; 8];
      let mut proj = [Point::new(0, 0); 8];
      for i in 0..8 {
        let [x, y, z] = VERTICES[i];
        let (x, y, z) = rotate_y(x, y, z, yaw);
        let (x, y, z) = rotate_x(x, y, z, pitch);
        let (x, y, z) = rotate_z(x, y, z, roll);
        cam[i] = to_camera([x, y, z]);
        proj[i] = project(cam[i]);
      }

      canvas.clear(Rgb565::BLACK);

      // Draw faces (backface culled, flat shaded)
      for face in FACES {
        let (a, b, c) = (cam[face[0]], cam[face[1]], cam[face[2]]);
        // Z component of the camera-space normal (screen-space signed area)
        let nz = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if nz <= 0.0 {
          continue;
        }

        // World-space normal for lighting
        let (ux, uy, uz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
        let (vx, vy, vz) = (c[0] - a[0], c[1] - a[1], c[2] - a[2]);
        let nx = uy * vz - uz * vy;
        let ny = uz * vx - ux * vz;
        let nz = ux * vy - uy * vx;
        let len = fast_sqrt(nx * nx + ny * ny + nz * nz);
        let intensity = if len > 0.0 {
          (nx * light[0] + ny * light[1] + nz * light[2]) / (len * light_len)
        } else {
          0.5
        };
        let intensity = (0.25 + 0.75 * intensity).clamp(0.25, 1.0);

        canvas.fill_triangle(proj[face[0]], proj[face[1]], proj[face[2]], shade(Rgb565::CYAN, intensity));
      }

      // Draw edges
      for (a, b) in EDGES {
        canvas.draw_line(proj[a], proj[b], Rgb565::WHITE);
      }

      unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

      yield_now().await;
    }
  })());
}
