// A cube with shaded sides

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
use embedded_graphics::{
  Drawable as _,
  pixelcolor::Rgb565,
  prelude::{Point, Primitive as _, RgbColor},
  primitives::{Line, PrimitiveStyle, Triangle},
};

static ANIMATION_DURATION: usize = 10_000;

// 3D vector operations
struct Vec3 {
  x: f32,
  y: f32,
  z: f32,
}

impl Vec3 {
  fn new(x: f32, y: f32, z: f32) -> Self {
    Vec3 { x, y, z }
  }

  fn rotate_x(&self, angle: f32) -> Vec3 {
    let cos_a = libm::cosf(angle);
    let sin_a = libm::sinf(angle);
    Vec3::new(self.x, self.y * cos_a - self.z * sin_a, self.y * sin_a + self.z * cos_a)
  }

  fn rotate_y(&self, angle: f32) -> Vec3 {
    let cos_a = libm::cosf(angle);
    let sin_a = libm::sinf(angle);
    Vec3::new(
      self.x * cos_a + self.z * sin_a,
      self.y,
      -self.x * sin_a + self.z * cos_a,
    )
  }

  fn rotate_z(&self, angle: f32) -> Vec3 {
    let cos_a = libm::cosf(angle);
    let sin_a = libm::sinf(angle);
    Vec3::new(self.x * cos_a - self.y * sin_a, self.x * sin_a + self.y * cos_a, self.z)
  }

  fn project(&self, scale: f32, offset_x: f32, offset_y: f32) -> Point {
    let perspective = 1.0 / (self.z + 4.0);
    Point::new(
      (self.x * scale * perspective + offset_x) as i32,
      (self.y * scale * perspective + offset_y) as i32,
    )
  }
}

// Calculate face normal for backface culling
fn face_normal(v0: &Vec3, v1: &Vec3, v2: &Vec3) -> f32 {
  let ux = v1.x - v0.x;
  let uy = v1.y - v0.y;
  let vx = v2.x - v0.x;
  let vy = v2.y - v0.y;
  ux * vy - uy * vx
}

// Calculate lighting intensity based on face normal
fn calculate_intensity(v0: &Vec3, v1: &Vec3, v2: &Vec3) -> f32 {
  let ux = v1.x - v0.x;
  let uy = v1.y - v0.y;
  let uz = v1.z - v0.z;
  let vx = v2.x - v0.x;
  let vy = v2.y - v0.y;
  let vz = v2.z - v0.z;

  // Cross product gives normal
  let nx = uy * vz - uz * vy;
  let ny = uz * vx - ux * vz;
  let nz = ux * vy - uy * vx;

  // Normalize
  let len = libm::sqrtf(nx * nx + ny * ny + nz * nz);
  if len < 0.001 {
    return 0.5;
  }

  let nx = nx / len;
  let ny = ny / len;
  let nz = nz / len;

  // Light direction (from top-right-front)
  let lx = 0.5f32;
  let ly = -0.5f32;
  let lz = -1.0f32;
  let llen = libm::sqrtf(lx * lx + ly * ly + lz * lz);

  // Dot product with light direction
  let intensity = (nx * lx + ny * ly + nz * lz) / llen;

  // Clamp between 0.2 and 1.0 for ambient + diffuse
  libm::fmaxf(0.2, libm::fminf(1.0, intensity))
}

fn color_from_intensity(intensity: f32, base_color: Rgb565) -> Rgb565 {
  let r = ((base_color.r() as f32 * intensity) as u8).min(31);
  let g = ((base_color.g() as f32 * intensity) as u8).min(63);
  let b = ((base_color.b() as f32 * intensity) as u8).min(31);
  Rgb565::new(r, g, b)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);
    let mut display = BufferTarget::new(buf);

    let center_x = SCREEN_WIDTH as f32 / 2.0;
    let center_y = SCREEN_HEIGHT as f32 / 2.0;
    let scale = 200.0;

    // Define cube vertices
    let cube_verts = [
      Vec3::new(-1.0, -1.0, -1.0), // 0
      Vec3::new(1.0, -1.0, -1.0),  // 1
      Vec3::new(1.0, 1.0, -1.0),   // 2
      Vec3::new(-1.0, 1.0, -1.0),  // 3
      Vec3::new(-1.0, -1.0, 1.0),  // 4
      Vec3::new(1.0, -1.0, 1.0),   // 5
      Vec3::new(1.0, 1.0, 1.0),    // 6
      Vec3::new(-1.0, 1.0, 1.0),   // 7
    ];

    // Define faces as triangles (2 triangles per face)
    // Format: (v0, v1, v2, base_color)
    let faces = [
      // Front face (red)
      (4, 5, 6, Rgb565::new(255, 0, 0)),
      (4, 6, 7, Rgb565::new(255, 0, 0)),
      // Back face (green)
      (1, 0, 3, Rgb565::new(0, 255, 0)),
      (1, 3, 2, Rgb565::new(0, 255, 0)),
      // Top face (blue)
      (3, 7, 6, Rgb565::new(0, 0, 255)),
      (3, 6, 2, Rgb565::new(0, 0, 255)),
      // Bottom face (yellow)
      (0, 1, 5, Rgb565::new(255, 255, 0)),
      (0, 5, 4, Rgb565::new(255, 255, 0)),
      // Right face (cyan)
      (1, 2, 6, Rgb565::new(0, 255, 255)),
      (1, 6, 5, Rgb565::new(0, 255, 255)),
      // Left face (magenta)
      (0, 4, 7, Rgb565::new(255, 0, 255)),
      (0, 7, 3, Rgb565::new(255, 0, 255)),
    ];

    let start = get_millis();
    let mut last_tick = start;

    loop {
      let now = get_millis();
      let elapsed = now - start;
      last_tick = now;

      if elapsed > ANIMATION_DURATION as u32 {
        break;
      }

      display.clear();

      // Calculate rotation angles
      let t = elapsed as f32 / 1000.0;
      let angle_x = t * 0.7;
      let angle_y = t * 1.0;
      let angle_z = t * 0.5;

      // Rotate and project vertices
      let mut projected: [Point; 8] = [Point::zero(); 8];
      let mut rotated: [Vec3; 8] = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
      ];

      for i in 0..8 {
        rotated[i] = cube_verts[i].rotate_x(angle_x).rotate_y(angle_y).rotate_z(angle_z);
        projected[i] = rotated[i].project(scale, center_x, center_y);
      }

      // Draw faces with shading
      for (v0_idx, v1_idx, v2_idx, base_color) in faces.iter() {
        let v0 = &rotated[*v0_idx];
        let v1 = &rotated[*v1_idx];
        let v2 = &rotated[*v2_idx];

        // Backface culling
        if face_normal(v0, v1, v2) < 0.0 {
          continue;
        }

        let intensity = calculate_intensity(v0, v1, v2) * 2.;
        let color = color_from_intensity(intensity, *base_color);

        Triangle::new(projected[*v0_idx], projected[*v1_idx], projected[*v2_idx])
          .into_styled(PrimitiveStyle::with_fill(color))
          .draw(&mut display)
          .unwrap();
      }

      // Draw edges
      let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0), // Back face
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4), // Front face
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7), // Connecting edges
      ];

      let edge_style = PrimitiveStyle::with_stroke(Rgb565::WHITE, 1);
      for (start_idx, end_idx) in edges.iter() {
        Line::new(projected[*start_idx], projected[*end_idx])
          .into_styled(edge_style)
          .draw(&mut display)
          .unwrap();
      }

      unsafe { extern_set_lcd_buffer(display.get_buffer_ptr()) };
      yield_now().await;
    }
  })());
}
