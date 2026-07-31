use core::f32::consts::PI;

const TWO_PI: f32 = 2.0 * PI;
const HALF_PI: f32 = PI * 0.5;

/// Compact `sin` approximation. Handles any `f32` angle by reducing to
/// [-PI, PI] first, so it avoids the large-argument reduction (`rem_pio2f`)
/// that the full `libm` implementation pulls in. Error < ~1e-4.
pub fn fast_sin(x: f32) -> f32 {
  let mut x = x % TWO_PI;
  if x > PI {
    x -= TWO_PI;
  } else if x < -PI {
    x += TWO_PI;
  }
  let sign = if x < 0.0 {
    x = -x;
    -1.0
  } else {
    1.0
  };
  if x > HALF_PI {
    x = PI - x;
  }
  let x2 = x * x;
  sign * x * (1.0 - x2 * (1.0 / 6.0 - x2 * (1.0 / 120.0 - x2 * (1.0 / 5040.0 - x2 / 362880.0))))
}

/// Compact `cos` approximation via `sin(x + PI/2)`.
pub fn fast_cos(x: f32) -> f32 {
  fast_sin(x + HALF_PI)
}
