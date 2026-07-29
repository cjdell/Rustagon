use crate::{
  apps::{MenuAppAsync, MenuAppContext, MenuAppInput, common::AppName},
  platform::Platform,
  types::*,
};
use alloc::{format, string::String, string::ToString, vec, vec::Vec};

pub struct InputTestApp<P: Platform> {
  ctx: MenuAppContext<P>,
  pressed: u32,
  remaining: u32,
}

impl<P: Platform> AppName for InputTestApp<P> {
  fn app_name() -> &'static str {
    "Input Test"
  }
}

fn button_label(btn: &HexButton) -> &'static str {
  match btn {
    HexButton::Up => "Up",
    HexButton::Down => "Down",
    HexButton::Left => "Left",
    HexButton::Right => "Right",
    HexButton::Fire => "Fire",
    HexButton::HexA => "Hex A",
    HexButton::HexB => "Hex B",
    HexButton::HexC => "Hex C",
    HexButton::HexD => "Hex D",
    HexButton::HexE => "Hex E",
    HexButton::HexF => "Hex F",
    HexButton::Touch01 => "Touch 01",
    HexButton::Touch02 => "Touch 02",
    HexButton::Touch03 => "Touch 03",
    HexButton::Touch04 => "Touch 04",
    HexButton::Touch05 => "Touch 05",
    HexButton::Touch06 => "Touch 06",
    HexButton::Touch07 => "Touch 07",
    HexButton::Touch08 => "Touch 08",
    HexButton::Touch09 => "Touch 09",
    HexButton::Touch10 => "Touch 10",
    HexButton::Touch11 => "Touch 11",
    HexButton::Touch12 => "Touch 12",
  }
}

const ALL_BUTTONS: &[HexButton] = &[
  HexButton::Up,
  HexButton::Down,
  HexButton::Left,
  HexButton::Right,
  HexButton::Fire,
  HexButton::HexA,
  HexButton::HexB,
  HexButton::HexC,
  HexButton::HexD,
  HexButton::HexE,
  HexButton::HexF,
  HexButton::Touch01,
  HexButton::Touch02,
  HexButton::Touch03,
  HexButton::Touch04,
  HexButton::Touch05,
  HexButton::Touch06,
  HexButton::Touch07,
  HexButton::Touch08,
  HexButton::Touch09,
  HexButton::Touch10,
  HexButton::Touch11,
  HexButton::Touch12,
];

fn bit(btn: &HexButton) -> u32 {
  for (i, b) in ALL_BUTTONS.iter().enumerate() {
    if b == btn {
      return 1u32 << i;
    }
  }
  0
}

fn render(pressed: u32) -> LcdScreen {
  let mut lines: Vec<MenuLine> = ALL_BUTTONS
    .iter()
    .filter(|btn| pressed & bit(btn) == 0)
    .map(|btn| MenuLine(Icon20::Info, button_label(btn).to_string()))
    .collect();

  let remaining = lines.len();

  let msg = if remaining == 0 {
    "All buttons work!".to_string()
  } else {
    format!("{remaining} buttons remaining")
  };
  LcdScreen::Menu { menu: lines, selected: 0 }
}

impl<P: Platform> InputTestApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self {
      ctx,
      pressed: 0,
      remaining: ALL_BUTTONS.len() as u32,
    }
  }
}

impl<P: Platform> MenuAppAsync for InputTestApp<P> {
  async fn work(&mut self) -> bool {
    let total = ALL_BUTTONS.len();
    loop {
      self.ctx.update_lcd(render(self.pressed));
      match self.ctx.input_receiver.receive().await {
        MenuAppInput::HexButton(input) => {
          self.pressed |= bit(&input);
          // Exit when every button has been pressed at least once
          if self.pressed.count_ones() as usize >= total {
            return false;
          }
        }
        MenuAppInput::Stop => return false,
        _ => {}
      }
    }
  }
}
