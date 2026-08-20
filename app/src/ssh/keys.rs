//! SSH-specific key → byte-sequence mappings for the remote shell.
//!
//! Kept out of the generic [`crate::ui::terminal::Terminal`] widget on
//! purpose: the terminal renders whatever stream it is fed, while *which*
//! bytes a key or badge button should send is an SSH-shell convention.

use crate::types::{HexButton, KeyCode};
use alloc::{vec, vec::Vec};

/// Map a keyboard key to the byte sequence to send to the remote shell.
///
/// Character keys produce their UTF-8 bytes (Shift-aware); editing keys produce
/// their control sequences. Directional keys and Enter never arrive here — the
/// platform unifies them into [`HexButton`] presses, so they are handled by
/// [`hex_button_to_bytes`].
pub fn key_to_bytes(code: KeyCode, shifted: bool) -> Option<Vec<u8>> {
  match code {
    KeyCode::Enter => Some(vec![b'\r']),
    KeyCode::Backspace => Some(vec![0x7f]),
    KeyCode::Tab => Some(vec![0x09]),
    KeyCode::Escape => Some(vec![0x1b]),
    _ => code.to_char(shifted).map(|ch| {
      let mut buf = [0u8; 4];
      let s = ch.encode_utf8(&mut buf);
      s.as_bytes().to_vec()
    }),
  }
}

/// Map a physical hex button to the byte sequence to send to the remote shell.
pub fn hex_button_to_bytes(button: HexButton) -> Option<Vec<u8>> {
  match button {
    HexButton::Up => Some(b"\x1b[A".to_vec()),
    HexButton::Down => Some(b"\x1b[B".to_vec()),
    HexButton::Right => Some(b"\x1b[C".to_vec()),
    HexButton::Left => Some(b"\x1b[D".to_vec()),
    HexButton::Fire => Some(vec![b'\r']),
    // Ctrl-characters on the function ring (like a mini keyboard).
    HexButton::HexA => Some(b"\x01".to_vec()), // Ctrl-A (line start)
    HexButton::HexB => Some(b"\x02".to_vec()), // Ctrl-B (back one char)
    HexButton::HexC => Some(b"\x03".to_vec()), // Ctrl-C (interrupt)
    HexButton::HexD => Some(b"\x04".to_vec()), // Ctrl-D (EOF)
    HexButton::HexE => Some(b"\x05".to_vec()), // Ctrl-E (line end)
    HexButton::HexF => Some(b"\x1b".to_vec()), // Escape
    _ => None,
  }
}
