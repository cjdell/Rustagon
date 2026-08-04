//! Keyboard domain logic shared by all apps: `KeyCode` → character conversion
//! including Shift handling.
//!
//! The platform (keyboard driver / desktop key mapping) only reports raw
//! `KeyCode`s, including `KeyCode::Shift` press/release. Shifting a character
//! is app-level domain logic, so it lives here in the `app` crate — a single
//! reusable source of truth that every app can call with its own shift state.

use crate::types::KeyCode;

/// Map a key's unshifted character to its shifted equivalent for keys that
/// shift to a symbol (digits and punctuation). Letters shift to their
/// uppercase form (handled in [`KeyCode::to_char`]), modifiers never produce a
/// character.
pub const SHIFTED_SYMBOL_MAP: &[(KeyCode, char)] = &[
  (KeyCode::Digit1, '!'),
  (KeyCode::Digit2, '@'),
  (KeyCode::Digit3, '#'),
  (KeyCode::Digit4, '$'),
  (KeyCode::Digit5, '%'),
  (KeyCode::Digit6, '^'),
  (KeyCode::Digit7, '&'),
  (KeyCode::Digit8, '*'),
  (KeyCode::Digit9, '('),
  (KeyCode::Digit0, ')'),
  (KeyCode::Minus, '_'),
  (KeyCode::Backtick, '~'),
  (KeyCode::Comma, '<'),
  (KeyCode::Period, '>'),
  (KeyCode::Slash, '?'),
  (KeyCode::Semicolon, ':'),
  (KeyCode::Quote, '"'),
  (KeyCode::Equals, '+'),
  (KeyCode::Backslash, '|'),
  (KeyCode::LBracket, '{'),
  (KeyCode::RBracket, '}'),
];

impl KeyCode {
  /// Convert a key to the character it produces on the given keyboard layout.
  ///
  /// Pass the caller's current shift state (tracked from `KeyCode::Shift`
  /// press/release events). When `shifted` is true, letters become uppercase
  /// and symbol keys (digits, punctuation) produce their shifted equivalent.
  /// Returns `None` for keys that do not produce a character (modifiers,
  /// navigation, editing keys, function keys).
  pub fn to_char(self, shifted: bool) -> Option<char> {
    match self {
      KeyCode::A => Some('a'),
      KeyCode::B => Some('b'),
      KeyCode::C => Some('c'),
      KeyCode::D => Some('d'),
      KeyCode::E => Some('e'),
      KeyCode::F => Some('f'),
      KeyCode::G => Some('g'),
      KeyCode::H => Some('h'),
      KeyCode::I => Some('i'),
      KeyCode::J => Some('j'),
      KeyCode::K => Some('k'),
      KeyCode::L => Some('l'),
      KeyCode::M => Some('m'),
      KeyCode::N => Some('n'),
      KeyCode::O => Some('o'),
      KeyCode::P => Some('p'),
      KeyCode::Q => Some('q'),
      KeyCode::R => Some('r'),
      KeyCode::S => Some('s'),
      KeyCode::T => Some('t'),
      KeyCode::U => Some('u'),
      KeyCode::V => Some('v'),
      KeyCode::W => Some('w'),
      KeyCode::X => Some('x'),
      KeyCode::Y => Some('y'),
      KeyCode::Z => Some('z'),
      KeyCode::Digit0 => Some('0'),
      KeyCode::Digit1 => Some('1'),
      KeyCode::Digit2 => Some('2'),
      KeyCode::Digit3 => Some('3'),
      KeyCode::Digit4 => Some('4'),
      KeyCode::Digit5 => Some('5'),
      KeyCode::Digit6 => Some('6'),
      KeyCode::Digit7 => Some('7'),
      KeyCode::Digit8 => Some('8'),
      KeyCode::Digit9 => Some('9'),
      KeyCode::Comma => Some(','),
      KeyCode::Period => Some('.'),
      KeyCode::Slash => Some('/'),
      KeyCode::Semicolon => Some(';'),
      KeyCode::Quote => Some('\''),
      KeyCode::Minus => Some('-'),
      KeyCode::Equals => Some('='),
      KeyCode::Backtick => Some('`'),
      KeyCode::Backslash => Some('\\'),
      KeyCode::LBracket => Some('['),
      KeyCode::RBracket => Some(']'),
      _ => return None,
    }
    .map(|ch| {
      if shifted {
        // Letters shift to uppercase; symbol keys shift via the symbol map.
        // `SHIFTED_SYMBOL_MAP` covers digits and punctuation, so anything not
        // in it (i.e. letters) uppercases.
        SHIFTED_SYMBOL_MAP
          .iter()
          .find(|(code, _)| *code == self)
          .map(|(_, ch)| *ch)
          .unwrap_or_else(|| ch.to_ascii_uppercase())
      } else {
        ch
      }
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn letters_shift_to_uppercase() {
    assert_eq!(KeyCode::A.to_char(false), Some('a'));
    assert_eq!(KeyCode::A.to_char(true), Some('A'));
    assert_eq!(KeyCode::Z.to_char(true), Some('Z'));
  }

  #[test]
  fn digits_shift_to_symbols() {
    assert_eq!(KeyCode::Digit1.to_char(true), Some('!'));
    assert_eq!(KeyCode::Digit0.to_char(true), Some(')'));
    assert_eq!(KeyCode::Digit5.to_char(true), Some('%'));
  }

  #[test]
  fn punctuation_shift() {
    assert_eq!(KeyCode::Semicolon.to_char(true), Some(':'));
    assert_eq!(KeyCode::Slash.to_char(true), Some('?'));
    assert_eq!(KeyCode::LBracket.to_char(true), Some('{'));
    assert_eq!(KeyCode::Quote.to_char(true), Some('"'));
  }

  #[test]
  fn unshifted_symbols_unchanged() {
    assert_eq!(KeyCode::Digit1.to_char(false), Some('1'));
    assert_eq!(KeyCode::Slash.to_char(false), Some('/'));
  }

  #[test]
  fn modifiers_and_nav_produce_nothing() {
    assert_eq!(KeyCode::Shift.to_char(true), None);
    assert_eq!(KeyCode::Ctrl.to_char(true), None);
    assert_eq!(KeyCode::Backspace.to_char(true), None);
    assert_eq!(KeyCode::Space.to_char(true), None);
  }
}
