//! Minimal integer formatting — a replacement for `alloc::format!` for
//! number-heavy apps so the (large) `core::fmt` machinery never gets linked.
//!
//! Only `core` is used: numbers are written into a caller-provided stack
//! buffer and returned as a `&str`.

use crate::helper::print_line;

/// Write `value` as decimal into `out` (right-aligned) and return the slice.
/// `out` must be large enough for the digits (10 for `u32`).
pub fn u32_to_str(mut value: u32, out: &mut [u8]) -> &str {
  let mut i = out.len();
  if value == 0 {
    i -= 1;
    out[i] = b'0';
  } else {
    while value > 0 {
      i -= 1;
      out[i] = b'0' + (value % 10) as u8;
      value /= 10;
    }
  }
  core::str::from_utf8(&out[i..]).unwrap()
}

/// Write `value` as decimal (with a `-` for negatives) into `out`.
/// `out` must be large enough for the sign + digits (12 for `i32`).
pub fn i32_to_str(value: i32, out: &mut [u8]) -> &str {
  if value >= 0 {
    return u32_to_str(value as u32, out);
  }
  let mut mag = (0i64 - value as i64) as u64;
  let mut i = out.len();
  while mag > 0 {
    i -= 1;
    out[i] = b'0' + (mag % 10) as u8;
    mag /= 10;
  }
  i -= 1;
  out[i] = b'-';
  core::str::from_utf8(&out[i..]).unwrap()
}

/// Write `value` as uppercase hex into `out` (right-aligned, no `0x`).
/// `out` must be large enough for the digits (8 for `u32`).
pub fn u32_to_hex(mut value: u32, out: &mut [u8]) -> &str {
  const HEX: &[u8; 16] = b"0123456789ABCDEF";
  let mut i = out.len();
  if value == 0 {
    i -= 1;
    out[i] = b'0';
  } else {
    while value > 0 {
      i -= 1;
      out[i] = HEX[(value & 0xF) as usize];
      value >>= 4;
    }
  }
  core::str::from_utf8(&out[i..]).unwrap()
}

/// Print `text` as-is (no trailing newline).
pub fn print_str(text: &str) {
  print_line(text);
}

/// Print a `u32` as decimal (no trailing newline).
pub fn print_u32(value: u32) {
  let mut buf = [0u8; 10];
  print_line(u32_to_str(value, &mut buf));
}

/// Print an `i32` as decimal (no trailing newline).
pub fn print_i32(value: i32) {
  let mut buf = [0u8; 12];
  print_line(i32_to_str(value, &mut buf));
}

/// Print a `u32` as uppercase hex (no trailing newline).
pub fn print_u32_hex(value: u32) {
  let mut buf = [0u8; 8];
  print_line(u32_to_hex(value, &mut buf));
}

/// Append `s` to `buf` at `*len`; returns false (and writes nothing) if it
/// would overflow the buffer.
pub fn append_str(buf: &mut [u8], len: &mut usize, s: &str) -> bool {
  if *len + s.len() > buf.len() {
    return false;
  }
  buf[*len..*len + s.len()].copy_from_slice(s.as_bytes());
  *len += s.len();
  true
}

/// Append `value` as decimal to `buf` at `*len`.
pub fn append_u32(buf: &mut [u8], len: &mut usize, value: u32) -> bool {
  let mut tmp = [0u8; 10];
  append_str(buf, len, u32_to_str(value, &mut tmp))
}

/// Append `value` as uppercase hex to `buf` at `*len`.
pub fn append_hex(buf: &mut [u8], len: &mut usize, value: u32) -> bool {
  let mut tmp = [0u8; 8];
  append_str(buf, len, u32_to_hex(value, &mut tmp))
}
