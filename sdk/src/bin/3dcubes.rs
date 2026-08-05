// Three rotating cubes with a directional light, rendered with the
// zero-dependency gfx library.
//
// Port of the embedded-3dgfx "3dcubes" sample: a fixed camera at (0, 3, 10)
// looking at the origin. Use Up/Down/Right to steer the light, Fire to toggle
// auto-rotation, any other button to exit. No embedded-graphics, embedded-3dgfx
// or nalgebra.

#![no_std]
#![no_main]

#[macro_use]
extern crate sdk;
use sdk as lib;

extern crate alloc;

use crate::lib::{
  fmt,
  gfx::{Canvas, Point, Rgb565, SCREEN_HEIGHT, SCREEN_WIDTH},
  helper::get_millis,
  protocol::{HexButton, HostIpcMessage, extern_set_lcd_buffer},
  tasks::{HOST_IPC_CHANNEL, spawn, yield_now},
  trig::{fast_cos, fast_sin, fast_sqrt},
};
use alloc::boxed::Box;

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

// Camera at (0, 3, 10) looking at the origin. Precomputed basis (scaled by
// 1/sqrt(109) and folded into FOCAL).
const FOCAL: f32 = 280.0;
const INV_SQRT109: f32 = 0.09578262665741734;

struct Cube {
  pos: [f32; 3],
  color: Rgb565,
  rates: [f32; 3], // yaw, pitch, roll per second
}

const CUBES: [Cube; 3] = [
  Cube {
    pos: [-3.0, 0.0, 0.0],
    color: Rgb565::new(31, 0, 0),
    rates: [0.3, 0.5, 0.2],
  },
  Cube {
    pos: [0.0, 0.0, 0.0],
    color: Rgb565::new(0, 63, 0),
    rates: [0.4, 0.3, 0.6],
  },
  Cube {
    pos: [3.0, 0.0, 0.0],
    color: Rgb565::new(0, 0, 31),
    rates: [0.5, 0.4, 0.3],
  },
];

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

/// Camera-space coordinates (y = up, z = forward).
fn to_camera(v: [f32; 3]) -> [f32; 3] {
  let dy = ((v[1] - 3.0) * 10.0 - (v[2] - 10.0) * 3.0) * INV_SQRT109;
  let dz = (-(v[1] - 3.0) * 3.0 - (v[2] - 10.0) * 10.0) * INV_SQRT109;
  [v[0], dy, dz]
}

/// Project a camera-space vertex to the screen.
fn project(cam: [f32; 3]) -> Point {
  Point::new(
    (cam[0] * FOCAL / cam[2] + SCREEN_WIDTH as f32 / 2.0) as i32,
    (-cam[1] * FOCAL / cam[2] + SCREEN_HEIGHT as f32 / 2.0) as i32,
  )
}

fn shade(base: Rgb565, intensity: f32) -> Rgb565 {
  Rgb565::new(
    (base.r5() as f32 * intensity) as u8,
    (base.g6() as f32 * intensity) as u8,
    (base.b5() as f32 * intensity) as u8,
  )
}

// (depth, color, a, b, c) — painter's-algorithm draw list.
type Face = (f32, Rgb565, Point, Point, Point);

#[unsafe(no_mangle)]
fn tick(host_msg_id: u32, host_msg_size: u32) -> bool {
  lib::tasks::runtime_tick(host_msg_id, host_msg_size)
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let mut buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);
    let mut canvas = Canvas::new(&mut buf[..], SCREEN_WIDTH, SCREEN_HEIGHT);

    let start_time = get_millis();
    let mut auto_rotate = true;
    let mut manual_light_angle_h = 0.0f32;
    let mut manual_light_angle_v = 0.0f32;

    canvas.clear(Rgb565::BLACK);
    unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

    let mut subscriber = log_error!(HOST_IPC_CHANNEL.subscriber(), "subscriber");

    'running: loop {
      // Handle events
      if let Some((_, host_ipc_msg)) = subscriber.try_next_message_pure() {
        match host_ipc_msg {
          HostIpcMessage::HexButton(hex_button) => match hex_button {
            HexButton::Up => {
              manual_light_angle_v += 0.1;
              println_light(&manual_light_angle_v);
            }
            HexButton::Right => {
              manual_light_angle_h += 0.2;
            }
            HexButton::Fire => {
              auto_rotate = !auto_rotate;
            }
            HexButton::Down => {
              manual_light_angle_v -= 0.1;
              println_light(&manual_light_angle_v);
            }
            HexButton::Left => {
              manual_light_angle_h -= 0.2;
            }
            _ => break 'running,
          },
          _ => {}
        }
      }

      // Light direction
      let time = (get_millis() - start_time) as f32 / 1_000.0;
      let light_h = if auto_rotate { time * 1.0 } else { manual_light_angle_h };
      let light_v = if auto_rotate {
        fast_sin(time * 0.5) * 0.3
      } else {
        manual_light_angle_v
      };
      let (lx, ly, lz) = (fast_cos(light_h), light_v, fast_sin(light_h));
      let llen = fast_sqrt(lx * lx + ly * ly + lz * lz);

      // Transform + project every cube
      let mut world = [[[0f32; 3]; 8]; 3];
      let mut cam = [[[0f32; 3]; 8]; 3];
      let mut proj = [[Point::new(0, 0); 8]; 3];
      for (ci, cube) in CUBES.iter().enumerate() {
        for i in 0..8 {
          let [x, y, z] = VERTICES[i];
          let (x, y, z) = rotate_y(x, y, z, time * cube.rates[0]);
          let (x, y, z) = rotate_x(x, y, z, time * cube.rates[1]);
          let (x, y, z) = rotate_z(x, y, z, time * cube.rates[2]);
          let w = [x + cube.pos[0], y + cube.pos[1], z + cube.pos[2]];
          world[ci][i] = w;
          cam[ci][i] = to_camera(w);
          proj[ci][i] = project(cam[ci][i]);
        }
      }

      // Collect visible, shaded faces
      let mut faces: [Face; 36] = [(0.0, Rgb565::BLACK, Point::new(0, 0), Point::new(0, 0), Point::new(0, 0)); 36];
      let mut n_faces = 0;
      for (ci, cube) in CUBES.iter().enumerate() {
        for face in FACES {
          let (a, b, c) = (proj[ci][face[0]], proj[ci][face[1]], proj[ci][face[2]]);
          // Backface cull (screen-space signed area; front faces wind < 0
          // with the y-flipped projection)
          let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
          if area >= 0 {
            continue;
          }

          // World-space normal for lighting
          let (v0, v1, v2) = (world[ci][face[0]], world[ci][face[1]], world[ci][face[2]]);
          let (ux, uy, uz) = (v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]);
          let (vx, vy, vz) = (v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]);
          let (nx, ny, nz) = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx);
          let len = fast_sqrt(nx * nx + ny * ny + nz * nz);
          let intensity = if len > 0.0 {
            (nx * lx + ny * ly + nz * lz) / (len * llen)
          } else {
            0.0
          };
          let intensity = (0.15 + 0.85 * intensity.max(0.0)).clamp(0.15, 1.0);

          let depth = (cam[ci][face[0]][2] + cam[ci][face[1]][2] + cam[ci][face[2]][2]) / 3.0;
          faces[n_faces] = (depth, shade(cube.color, intensity), a, b, c);
          n_faces += 1;
        }
      }

      // Painter's sort: far faces first (insertion sort, descending depth)
      for i in 1..n_faces {
        let key = faces[i];
        let mut j = i;
        while j > 0 && faces[j - 1].0 < key.0 {
          faces[j] = faces[j - 1];
          j -= 1;
        }
        faces[j] = key;
      }

      canvas.clear(Rgb565::BLACK);

      for i in 0..n_faces {
        let (_, color, a, b, c) = faces[i];
        canvas.fill_triangle(a, b, c, color);
      }

      // HUD
      let mut hud = [0u8; 40];
      let mut len = 0;
      fmt::append_str(&mut hud, &mut len, if auto_rotate { "AUTO ON" } else { "AUTO OFF" });
      fmt::append_str(&mut hud, &mut len, " H:");
      fmt::append_u32(&mut hud, &mut len, (light_h * 10.0) as i32 as u32);
      fmt::append_str(&mut hud, &mut len, " V:");
      fmt::append_u32(&mut hud, &mut len, (light_v * 10.0) as i32 as u32);
      let hud = core::str::from_utf8(&hud[..len]).unwrap();
      canvas.draw_text(hud, 4, 2, Rgb565::WHITE, 1);

      // Light direction indicator (top-right)
      let (icx, icy, ir) = (204, 30, 22);
      canvas.draw_circle(Point::new(icx, icy), ir, Rgb565::LIGHT_GRAY);
      canvas.draw_line(Point::new(icx - 7, icy), Point::new(icx + 7, icy), Rgb565::DARK_GRAY);
      canvas.draw_line(Point::new(icx, icy - 7), Point::new(icx, icy + 7), Rgb565::DARK_GRAY);
      let px = icx + (lx / llen * ir as f32) as i32;
      let py = icy - (lz / llen * ir as f32) as i32;
      canvas.draw_line(Point::new(icx, icy), Point::new(px, py), Rgb565::YELLOW);

      unsafe { extern_set_lcd_buffer(canvas.as_ptr()) };

      yield_now().await;
    }
  })());
}

fn println_light(angle_v: &f32) {
  let mut buf = [0u8; 16];
  fmt::print_str("Light height: ");
  let mut len = 0;
  fmt::append_u32(&mut buf, &mut len, (*angle_v * 10.0) as i32 as u32);
  crate::lib::helper::print_line(core::str::from_utf8(&buf[..len]).unwrap());
  crate::lib::helper::print_line("\n");
}
