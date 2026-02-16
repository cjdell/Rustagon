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
use alloc::vec;
use alloc::{boxed::Box, vec::Vec};
use core::iter::once;
use embedded_3dgfx::{
  K3dengine,
  draw::draw,
  mesh::{Geometry, K3dMesh, RenderMode},
};
use embedded_graphics::{pixelcolor::Rgb565, prelude::WebColors};
use nalgebra::Point3;

static ANIMATION_DURATION: usize = 5_000;

fn make_cube_vertices() -> Vec<[f32; 3]> {
  vec![
    // Front face
    [-1.0, -1.0, 1.0],
    [1.0, -1.0, 1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 1.0, 1.0],
    // Back face
    [-1.0, -1.0, -1.0],
    [1.0, -1.0, -1.0],
    [1.0, 1.0, -1.0],
    [-1.0, 1.0, -1.0],
  ]
}

fn make_cube_faces() -> Vec<[usize; 3]> {
  vec![
    // Front
    [0, 1, 2],
    [0, 2, 3],
    // Back
    [5, 4, 7],
    [5, 7, 6],
    // Top
    [3, 2, 6],
    [3, 6, 7],
    // Bottom
    [4, 5, 1],
    [4, 1, 0],
    // Right
    [1, 5, 6],
    [1, 6, 2],
    // Left
    [4, 0, 3],
    [4, 3, 7],
  ]
}

#[unsafe(no_mangle)]
fn wasm_main() {
  spawn((async || {
    let buf = Box::new([0x00u8; SCREEN_WIDTH * SCREEN_HEIGHT * 2]);

    let mut display = BufferTarget::new(buf);

    let start = get_millis();
    let mut last_tick = start;

    // Create 3D engine
    let mut engine = K3dengine::new(SCREEN_WIDTH as u16, SCREEN_HEIGHT as u16);
    engine.camera.set_position(Point3::new(0.0, 2.0, 3.0));
    engine.camera.set_target(Point3::new(0.0, 0.0, 0.0));

    // Create cube mesh
    let vertices = make_cube_vertices();
    let faces = make_cube_faces();

    let geometry = Geometry {
      vertices: &vertices,
      faces: &faces,
      colors: &[],
      lines: &[],
      normals: &[],
      uvs: &[],
      texture_id: None,
    };

    let mut cube = K3dMesh::new(geometry);
    cube.set_color(Rgb565::CSS_CYAN);

    let mut current_mode = 2;
    let modes = [
      ("Points", RenderMode::Points),
      ("Lines", RenderMode::Lines),
      ("Solid", RenderMode::Solid),
    ];

    cube.set_render_mode(modes[current_mode].1.clone());

    // Initial render
    display.clear();
    engine.render(once(&cube), |prim| {
      draw(prim, &mut display);
    });
    unsafe { extern_set_lcd_buffer(display.get_buffer_ptr()) };

    loop {
      let now = get_millis();
      let delta = now - last_tick;
      let elapsed = now - start;
      last_tick = now;

      // 5 second animation duration
      if now - start > ANIMATION_DURATION as u32 {
        break;
      }

      println!("delta: {delta}");

      // Clear display
      display.clear();

      // Render the cube
      engine.render(once(&cube), |prim| {
        draw(prim, &mut display);
      });

      let elapsed = elapsed as f32 / 1000.;
      cube.set_attitude(elapsed * 0.5, elapsed, elapsed * 0.3);

      // Update window
      unsafe { extern_set_lcd_buffer(display.get_buffer_ptr()) };

      yield_now().await;
    }
  })());
}
