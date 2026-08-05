//! Zero-dependency drawing primitives: a raw RGB565 framebuffer plus
//! line/rect/circle/triangle routines.
//!
//! Only `core` is used — no `embedded_graphics`, no `libm`, no `alloc`. The
//! framebuffer layout matches what the host LCD expects (RGB565, big-endian
//! bytes — the same layout the host LCD uses), so a canvas can be handed
//! straight to `extern_set_lcd_buffer`.

/// Logical screen width in pixels.
pub const SCREEN_WIDTH: usize = 240;
/// Logical screen height in pixels.
pub const SCREEN_HEIGHT: usize = 240;

/// 16-bit RGB565 color.
///
/// Construct with either raw 5/6/5 components ([`new`](Rgb565::new), matching
/// `embedded_graphics::Rgb565::new`) or 8-bit-per-channel components
/// ([`from_rgb888`](Rgb565::from_rgb888)).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb565(u16);

impl Rgb565 {
  pub const BLACK: Rgb565 = Rgb565(0x0000);
  pub const WHITE: Rgb565 = Rgb565(0xFFFF);
  pub const RED: Rgb565 = Rgb565(0xF800);
  pub const GREEN: Rgb565 = Rgb565(0x07E0);
  pub const BLUE: Rgb565 = Rgb565(0x001F);
  pub const YELLOW: Rgb565 = Rgb565(0xFFE0);
  pub const CYAN: Rgb565 = Rgb565(0x07FF);
  pub const MAGENTA: Rgb565 = Rgb565(0xF81F);
  pub const ORANGE: Rgb565 = Rgb565(0xFD20);
  pub const GRAY: Rgb565 = Rgb565(0x8410);
  pub const DARK_GRAY: Rgb565 = Rgb565(0x4208);
  pub const LIGHT_GRAY: Rgb565 = Rgb565(0xC618);

  /// Raw 5/6/5 components: `r` in 0-31, `g` in 0-63, `b` in 0-31.
  pub const fn new(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565(((r as u16 & 0x1F) << 11) | ((g as u16 & 0x3F) << 5) | (b as u16 & 0x1F))
  }

  /// 8-bit-per-channel components (`r`/`g`/`b` in 0-255), quantized to 5/6/5.
  pub const fn from_rgb888(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565(((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3))
  }

  /// The raw 16-bit RGB565 value.
  pub const fn raw(self) -> u16 {
    self.0
  }

  /// 5-bit red component (0-31).
  pub const fn r5(self) -> u8 {
    (self.0 >> 11) as u8
  }

  /// 6-bit green component (0-63).
  pub const fn g6(self) -> u8 {
    ((self.0 >> 5) & 0x3F) as u8
  }

  /// 5-bit blue component (0-31).
  pub const fn b5(self) -> u8 {
    (self.0 & 0x1F) as u8
  }
}

impl Default for Rgb565 {
  fn default() -> Self {
    Rgb565::BLACK
  }
}

/// A point in canvas coordinates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Point {
  pub x: i32,
  pub y: i32,
}

impl Point {
  pub const fn new(x: i32, y: i32) -> Point {
    Point { x, y }
  }
}

/// An axis-aligned rectangle (width/height, not bottom-right corner).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
  pub x: i32,
  pub y: i32,
  pub w: i32,
  pub h: i32,
}

impl Rect {
  pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
  }

  /// Rightmost pixel column (inclusive).
  pub const fn right(&self) -> i32 {
    self.x + self.w - 1
  }

  /// Bottom pixel row (inclusive).
  pub const fn bottom(&self) -> i32 {
    self.y + self.h - 1
  }
}

/// A drawable framebuffer of RGB565 pixels in big-endian byte order.
///
/// All drawing is clipped to the canvas bounds; out-of-bounds pixels are
/// silently ignored.
pub struct Canvas<'a> {
  buf: &'a mut [u8],
  w: usize,
  h: usize,
}

impl<'a> Canvas<'a> {
  /// Wrap `buf` as a `w` x `h` RGB565 framebuffer.
  ///
  /// # Panics
  ///
  /// Panics if `buf.len() < w * h * 2`.
  pub fn new(buf: &'a mut [u8], w: usize, h: usize) -> Canvas<'a> {
    assert!(buf.len() >= w * h * 2, "canvas buffer too small");
    Canvas { buf, w, h }
  }

  pub fn width(&self) -> usize {
    self.w
  }

  pub fn height(&self) -> usize {
    self.h
  }

  /// Pointer to the raw framebuffer, for `extern_set_lcd_buffer`.
  pub fn as_ptr(&self) -> *const u8 {
    self.buf.as_ptr()
  }

  /// Fill the whole canvas with `color`.
  pub fn clear(&mut self, color: Rgb565) {
    let hi = (color.0 >> 8) as u8;
    let lo = (color.0 & 0xFF) as u8;
    let mut i = 0;
    while i < self.buf.len() {
      self.buf[i] = hi;
      self.buf[i + 1] = lo;
      i += 2;
    }
  }

  /// Set a single pixel (no-op if out of bounds).
  #[inline]
  pub fn set_pixel(&mut self, x: i32, y: i32, color: Rgb565) {
    if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
      return;
    }
    let i = ((y as usize) * self.w + x as usize) * 2;
    self.buf[i] = (color.0 >> 8) as u8;
    self.buf[i + 1] = (color.0 & 0xFF) as u8;
  }

  /// Read a single pixel, or `None` if out of bounds.
  pub fn read_pixel(&self, x: i32, y: i32) -> Option<Rgb565> {
    if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
      return None;
    }
    let i = ((y as usize) * self.w + x as usize) * 2;
    Some(Rgb565(((self.buf[i] as u16) << 8) | self.buf[i + 1] as u16))
  }

  /// Bresenham line from `p0` to `p1`.
  pub fn draw_line(&mut self, p0: Point, p1: Point, color: Rgb565) {
    let mut x0 = p0.x;
    let mut y0 = p0.y;
    let x1 = p1.x;
    let y1 = p1.y;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
      self.set_pixel(x0, y0, color);
      if x0 == x1 && y0 == y1 {
        break;
      }
      let e2 = 2 * err;
      if e2 >= dy {
        err += dy;
        x0 += sx;
      }
      if e2 <= dx {
        err += dx;
        y0 += sy;
      }
    }
  }

  /// Rectangle outline.
  pub fn draw_rect(&mut self, r: Rect, color: Rgb565) {
    if r.w <= 0 || r.h <= 0 {
      return;
    }
    let x1 = r.right();
    let y1 = r.bottom();
    self.draw_line(Point::new(r.x, r.y), Point::new(x1, r.y), color);
    self.draw_line(Point::new(x1, r.y), Point::new(x1, y1), color);
    self.draw_line(Point::new(x1, y1), Point::new(r.x, y1), color);
    self.draw_line(Point::new(r.x, y1), Point::new(r.x, r.y), color);
  }

  /// Solid rectangle (clipped to the canvas).
  pub fn fill_rect(&mut self, r: Rect, color: Rgb565) {
    let x0 = r.x.max(0) as usize;
    let y0 = r.y.max(0) as usize;
    let x1 = (r.x + r.w).min(self.w as i32).max(0) as usize;
    let y1 = (r.y + r.h).min(self.h as i32).max(0) as usize;
    if x0 >= x1 || y0 >= y1 {
      return;
    }
    let hi = (color.0 >> 8) as u8;
    let lo = (color.0 & 0xFF) as u8;
    for y in y0..y1 {
      let mut i = (y * self.w + x0) * 2;
      let end = (y * self.w + x1) * 2;
      while i < end {
        self.buf[i] = hi;
        self.buf[i + 1] = lo;
        i += 2;
      }
    }
  }

  /// Circle outline (midpoint algorithm).
  pub fn draw_circle(&mut self, c: Point, radius: i32, color: Rgb565) {
    if radius < 0 {
      return;
    }
    if radius == 0 {
      self.set_pixel(c.x, c.y, color);
      return;
    }
    let mut x = radius;
    let mut y = 0;
    let mut err = 1 - radius;
    while x >= y {
      self.set_pixel(c.x + x, c.y + y, color);
      self.set_pixel(c.x + y, c.y + x, color);
      self.set_pixel(c.x - y, c.y + x, color);
      self.set_pixel(c.x - x, c.y + y, color);
      self.set_pixel(c.x - x, c.y - y, color);
      self.set_pixel(c.x - y, c.y - x, color);
      self.set_pixel(c.x + y, c.y - x, color);
      self.set_pixel(c.x + x, c.y - y, color);
      y += 1;
      if err < 0 {
        err += 2 * y + 1;
      } else {
        x -= 1;
        err += 2 * (y - x) + 1;
      }
    }
  }

  /// Solid circle (horizontal scanlines of the midpoint circle).
  pub fn fill_circle(&mut self, c: Point, radius: i32, color: Rgb565) {
    if radius < 0 {
      return;
    }
    let r2 = radius * radius;
    for dy in -radius..=radius {
      let chord = (r2 - dy * dy).isqrt();
      self.fill_rect(Rect::new(c.x - chord, c.y + dy, chord * 2 + 1, 1), color);
    }
  }

  /// Triangle outline.
  pub fn draw_triangle(&mut self, a: Point, b: Point, c: Point, color: Rgb565) {
    self.draw_line(a, b, color);
    self.draw_line(b, c, color);
    self.draw_line(c, a, color);
  }

  /// Solid triangle (barycentric rasterization over the bounding box).
  pub fn fill_triangle(&mut self, a: Point, b: Point, c: Point, color: Rgb565) {
    let min_x = a.x.min(b.x).min(c.x);
    let max_x = a.x.max(b.x).max(c.x);
    let min_y = a.y.min(b.y).min(c.y);
    let max_y = a.y.max(b.y).max(c.y);
    for y in min_y..=max_y {
      for x in min_x..=max_x {
        if point_in_triangle(x, y, a, b, c) {
          self.set_pixel(x, y, color);
        }
      }
    }
  }

  /// Copy an RGB565 image (big-endian bytes, `w * h * 2` long) to `(x, y)`.
  pub fn blit(&mut self, x: i32, y: i32, w: usize, h: usize, src: &[u8]) {
    let x0 = x.max(0) as usize;
    let y0 = y.max(0) as usize;
    let x1 = (x + w as i32).min(self.w as i32).max(0) as usize;
    let y1 = (y + h as i32).min(self.h as i32).max(0) as usize;
    if x0 >= x1 || y0 >= y1 {
      return;
    }
    for yy in y0..y1 {
      let src_row = (yy as i32 - y) as usize;
      let mut si = (src_row * w + (x0 as i32 - x) as usize) * 2;
      let mut di = (yy * self.w + x0) * 2;
      for _ in x0..x1 {
        self.buf[di] = src[si];
        self.buf[di + 1] = src[si + 1];
        si += 2;
        di += 2;
      }
    }
  }
}

/// Signed area test — which side of the edge `a -> b` is `(px, py)` on.
fn sign(px: i32, py: i32, a: Point, b: Point) -> i32 {
  (px - b.x) * (a.y - b.y) - (a.x - b.x) * (py - b.y)
}

/// Barycentric point-in-triangle test; points exactly on an edge count as
/// inside (so adjacent filled triangles share their edge pixels).
fn point_in_triangle(px: i32, py: i32, a: Point, b: Point, c: Point) -> bool {
  let d1 = sign(px, py, a, b);
  let d2 = sign(px, py, b, c);
  let d3 = sign(px, py, c, a);
  let has_neg = (d1 < 0) || (d2 < 0) || (d3 < 0);
  let has_pos = (d1 > 0) || (d2 > 0) || (d3 > 0);
  !(has_neg && has_pos)
}
