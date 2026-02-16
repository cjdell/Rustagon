use crate::{
  lib::{LedReceiver, LedRequest, NUM_LEDS},
  utils::led_service::{LedService, LedState},
};
use alloc::boxed::Box;
use alloc::vec;
use embassy_time::{Duration, Timer};
use esp_hal::time::Instant;
use esp_println::println;
use log::info;

#[embassy_executor::task]
pub async fn led_task(led_receiver: LedReceiver) {
  info!("Starting LED Task...");

  let mut led_service = LedService::new();

  // Start with off effect
  let mut current_effect: Box<dyn LedEffect> = Box::new(SolidEffect {
    colour: LedState { r: 255, g: 0, b: 0 },
  });

  let mut counter = 0;

  loop {
    // Check for new requests (non-blocking)
    if let Ok(new_request) = led_receiver.try_receive() {
      println!("new_request: {new_request:?}");

      current_effect = match new_request {
        LedRequest::Off => Box::new(OffEffect),
        LedRequest::Solid(led_state) => Box::new(SolidEffect { colour: led_state }),
        LedRequest::Rainbow => Box::new(RainbowEffect::new(0.1)), // 0.1 degrees per ms
        LedRequest::Breathe(led_state) => Box::new(BreatheEffect::new(led_state, 0.001)),
        LedRequest::Chase(led_state) => Box::new(ChaseEffect::new(led_state, 5, 50)),
        LedRequest::Sparkle(led_state) => Box::new(SparkleEffect::new(led_state, 0.3, 100)),
        LedRequest::TheaterChase(led_state) => Box::new(TheaterChaseEffect::new(led_state, 3, 100)),
        LedRequest::Fire => Box::new(FireEffect::new(55, 120, 30)),
      };
    }

    let now_ms: u64 = Instant::now().duration_since_epoch().as_millis();

    // Update and render current effect
    // The borrow is released at the end of this statement
    let states = current_effect.update_and_render(now_ms);

    let internal_led = if counter % 2 == 0 {
      LedState::new(255, 0, 0)
    } else {
      LedState::new(0, 0, 255)
    };

    // Total of 13 in the strip
    let states = vec![[internal_led].to_vec(), states.to_vec()].concat();

    led_service.send(&states).await;

    Timer::after(Duration::from_millis(10)).await;

    counter += 1;
  }
}

/// Trait for LED effects
pub trait LedEffect {
  /// Update effect state and render to LEDs
  fn update_and_render(&mut self, now_ms: u64) -> [LedState; NUM_LEDS];
}

pub struct OffEffect;

impl LedEffect for OffEffect {
  fn update_and_render(&mut self, _now_ms: u64) -> [LedState; NUM_LEDS] {
    [LedState::new(0, 0, 0); NUM_LEDS]
  }
}

pub struct SolidEffect {
  colour: LedState,
}

impl LedEffect for SolidEffect {
  fn update_and_render(&mut self, _now_ms: u64) -> [LedState; NUM_LEDS] {
    [self.colour; NUM_LEDS]
  }
}

/// Rainbow effect that cycles through colors
pub struct RainbowEffect {
  offset: f32,
  speed: f32, // degrees per millisecond
  last_update_ms: u64,
}

impl RainbowEffect {
  pub fn new(speed: f32) -> Self {
    Self {
      offset: 0.0,
      speed,
      last_update_ms: 0,
    }
  }
}

impl LedEffect for RainbowEffect {
  fn update_and_render(&mut self, now_ms: u64) -> [LedState; NUM_LEDS] {
    // Update offset based on elapsed time
    let delta_ms = if self.last_update_ms == 0 {
      0
    } else {
      now_ms - self.last_update_ms
    };
    self.last_update_ms = now_ms;

    self.offset = (self.offset + self.speed * delta_ms as f32) % 360.0;

    let mut states = [LedState::new(0, 0, 0); NUM_LEDS];
    for i in 0..NUM_LEDS {
      let hue = (self.offset + (i as f32 / NUM_LEDS as f32) * 360.0) % 360.0;
      let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
      states[i] = LedState::new(r, g, b);
    }
    states
  }
}

/// Breathing/pulsing effect
pub struct BreatheEffect {
  colour: LedState,
  brightness: f32,
  direction: f32, // 1.0 for brightening, -1.0 for dimming
  speed: f32,     // brightness change per millisecond
  last_update_ms: u64,
}

impl BreatheEffect {
  pub fn new(colour: LedState, speed: f32) -> Self {
    Self {
      colour,
      brightness: 0.0,
      direction: 1.0,
      speed,
      last_update_ms: 0,
    }
  }
}

impl LedEffect for BreatheEffect {
  fn update_and_render(&mut self, now_ms: u64) -> [LedState; NUM_LEDS] {
    let delta_ms = if self.last_update_ms == 0 {
      0
    } else {
      now_ms - self.last_update_ms
    };
    self.last_update_ms = now_ms;

    // Update brightness
    self.brightness += self.direction * self.speed * delta_ms as f32;

    // Reverse direction at bounds
    if self.brightness >= 1.0 {
      self.brightness = 1.0;
      self.direction = -1.0;
    } else if self.brightness <= 0.0 {
      self.brightness = 0.0;
      self.direction = 1.0;
    }

    let r = (self.colour.r as f32 * self.brightness) as u8;
    let g = (self.colour.g as f32 * self.brightness) as u8;
    let b = (self.colour.b as f32 * self.brightness) as u8;

    [LedState::new(r, g, b); NUM_LEDS]
  }
}

/// Chase effect - a dot moving along the strip
pub struct ChaseEffect {
  colour: LedState,
  position: usize,
  tail_length: usize,
  update_interval_ms: u64,
  last_update_ms: u64,
}

impl ChaseEffect {
  pub fn new(colour: LedState, tail_length: usize, speed_ms: u64) -> Self {
    Self {
      colour,
      position: 0,
      tail_length,
      update_interval_ms: speed_ms,
      last_update_ms: 0,
    }
  }
}

impl LedEffect for ChaseEffect {
  fn update_and_render(&mut self, now_ms: u64) -> [LedState; NUM_LEDS] {
    // Update position
    if now_ms - self.last_update_ms >= self.update_interval_ms {
      self.position = (self.position + 1) % NUM_LEDS;
      self.last_update_ms = now_ms;
    }

    let mut states = [LedState::new(0, 0, 0); NUM_LEDS];

    // Draw tail
    for i in 0..self.tail_length {
      let pos = (self.position + NUM_LEDS - i) % NUM_LEDS;
      let brightness = 1.0 - (i as f32 / self.tail_length as f32);
      let r = (self.colour.r as f32 * brightness) as u8;
      let g = (self.colour.g as f32 * brightness) as u8;
      let b = (self.colour.b as f32 * brightness) as u8;
      states[pos] = LedState::new(r, g, b);
    }

    states
  }
}

/// Sparkle effect - random LEDs twinkle
pub struct SparkleEffect {
  colour: LedState,
  density: f32, // 0.0 to 1.0, probability of LED being on
  update_interval_ms: u64,
  last_update_ms: u64,
  states: [LedState; NUM_LEDS],
}

impl SparkleEffect {
  pub fn new(colour: LedState, density: f32, speed_ms: u64) -> Self {
    Self {
      colour,
      density,
      update_interval_ms: speed_ms,
      last_update_ms: 0,
      states: [LedState::new(0, 0, 0); NUM_LEDS],
    }
  }
}

impl LedEffect for SparkleEffect {
  fn update_and_render(&mut self, now_ms: u64) -> [LedState; NUM_LEDS] {
    if now_ms - self.last_update_ms >= self.update_interval_ms {
      // Simple pseudo-random (good enough for LEDs)
      let mut seed = now_ms as u32;
      for i in 0..NUM_LEDS {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let random = (seed / 65536) % 100;

        if random < (self.density * 100.0) as u32 {
          self.states[i] = self.colour;
        } else {
          self.states[i] = LedState::new(0, 0, 0);
        }
      }
      self.last_update_ms = now_ms;
    }

    self.states
  }
}

/// Theater chase effect - blocks of color moving
pub struct TheaterChaseEffect {
  colour: LedState,
  block_size: usize,
  position: usize,
  update_interval_ms: u64,
  last_update_ms: u64,
}

impl TheaterChaseEffect {
  pub fn new(colour: LedState, block_size: usize, speed_ms: u64) -> Self {
    Self {
      colour,
      block_size,
      position: 0,
      update_interval_ms: speed_ms,
      last_update_ms: 0,
    }
  }
}

impl LedEffect for TheaterChaseEffect {
  fn update_and_render(&mut self, now_ms: u64) -> [LedState; NUM_LEDS] {
    if now_ms - self.last_update_ms >= self.update_interval_ms {
      self.position = (self.position + 1) % (self.block_size * 2);
      self.last_update_ms = now_ms;
    }

    let mut states = [LedState::new(0, 0, 0); NUM_LEDS];
    for i in 0..NUM_LEDS {
      if (i + self.position) % (self.block_size * 2) < self.block_size {
        states[i] = self.colour;
      }
    }
    states
  }
}

/// Fire effect - flickering warm colors
pub struct FireEffect {
  cooling: u8,  // How much each LED cools per step (higher = cooler)
  sparking: u8, // Chance of new spark (0-255)
  heat: [u8; NUM_LEDS],
  update_interval_ms: u64,
  last_update_ms: u64,
}

impl FireEffect {
  pub fn new(cooling: u8, sparking: u8, speed_ms: u64) -> Self {
    Self {
      cooling,
      sparking,
      heat: [0; NUM_LEDS],
      update_interval_ms: speed_ms,
      last_update_ms: 0,
    }
  }
}

impl LedEffect for FireEffect {
  fn update_and_render(&mut self, now_ms: u64) -> [LedState; NUM_LEDS] {
    if now_ms - self.last_update_ms >= self.update_interval_ms {
      let mut seed = now_ms as u32;

      // Cool down each LED
      for i in 0..NUM_LEDS {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let cooldown = ((seed / 65536) % self.cooling as u32) as u8;
        self.heat[i] = self.heat[i].saturating_sub(cooldown);
      }

      // Heat from each cell drifts up
      for i in (2..NUM_LEDS).rev() {
        self.heat[i] = ((self.heat[i - 1] as u16 + self.heat[i - 2] as u16) / 2) as u8;
      }

      // Randomly ignite new sparks near bottom
      seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
      if (seed % 255) < self.sparking as u32 {
        let pos = (seed % 7) as usize;
        self.heat[pos] = self.heat[pos].saturating_add(160).min(255);
      }

      self.last_update_ms = now_ms;
    }

    let mut states = [LedState::new(0, 0, 0); NUM_LEDS];
    for i in 0..NUM_LEDS {
      states[i] = heat_color(self.heat[i]);
    }
    states
  }
}

// Helper functions
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
  let c = v * s;
  let h_prime = h / 60.0;
  let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
  let m = v - c;

  let (r, g, b) = match h_prime as i32 {
    0 => (c, x, 0.0),
    1 => (x, c, 0.0),
    2 => (0.0, c, x),
    3 => (0.0, x, c),
    4 => (x, 0.0, c),
    _ => (c, 0.0, x),
  };

  (
    ((r + m) * 255.0) as u8,
    ((g + m) * 255.0) as u8,
    ((b + m) * 255.0) as u8,
  )
}

fn heat_color(temperature: u8) -> LedState {
  // Scale from black to red to yellow to white
  let t192 = ((temperature as u16 * 192) / 255) as u8;

  let heatramp = t192 & 0x3F; // 0..63
  let heatramp_scaled = heatramp << 2; // scale up to 0..252

  if t192 < 64 {
    // Black to red
    LedState::new(heatramp_scaled, 0, 0)
  } else if t192 < 128 {
    // Red to yellow
    LedState::new(255, heatramp_scaled, 0)
  } else {
    // Yellow to white
    LedState::new(255, 255, heatramp_scaled)
  }
}

// loop {
//   led_service.send(&[LedState::new(255, 0, 0); NUM_LEDS]).await;
//   Timer::after(Duration::from_millis(1_000)).await;

//   led_service.send(&[LedState::new(0, 255, 0); NUM_LEDS]).await;
//   Timer::after(Duration::from_millis(1_000)).await;
// }
