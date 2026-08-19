use crate::{
  apps::{AppAction, AppInput, AppRunContext, AppRunEvent, MenuApp, MenuAppContext, common::AppName},
  platform::Platform,
  types::*,
};
use alloc::{format, string::ToString, vec::Vec};

pub struct InputTestApp<P: Platform> {
  ctx: MenuAppContext<P>,
  pressed: u32,
}

impl<P: Platform> AppName for InputTestApp<P> {
  fn app_name() -> &'static str {
    "Input Test"
  }
}

fn button_label(btn: &HexButton) -> &'static str {
  match btn {
    HexButton::Up | HexButton::UpReleased => "Up",
    HexButton::Down | HexButton::DownReleased => "Down",
    HexButton::Left | HexButton::LeftReleased => "Left",
    HexButton::Right | HexButton::RightReleased => "Right",
    HexButton::Fire | HexButton::FireReleased => "Fire",
    HexButton::HexA | HexButton::HexAReleased => "Hex A",
    HexButton::HexB | HexButton::HexBReleased => "Hex B",
    HexButton::HexC | HexButton::HexCReleased => "Hex C",
    HexButton::HexD | HexButton::HexDReleased => "Hex D",
    HexButton::HexE | HexButton::HexEReleased => "Hex E",
    HexButton::HexF | HexButton::HexFReleased => "Hex F",
    HexButton::Touch01 | HexButton::Touch01Released => "Touch 01",
    HexButton::Touch02 | HexButton::Touch02Released => "Touch 02",
    HexButton::Touch03 | HexButton::Touch03Released => "Touch 03",
    HexButton::Touch04 | HexButton::Touch04Released => "Touch 04",
    HexButton::Touch05 | HexButton::Touch05Released => "Touch 05",
    HexButton::Touch06 | HexButton::Touch06Released => "Touch 06",
    HexButton::Touch07 | HexButton::Touch07Released => "Touch 07",
    HexButton::Touch08 | HexButton::Touch08Released => "Touch 08",
    HexButton::Touch09 | HexButton::Touch09Released => "Touch 09",
    HexButton::Touch10 | HexButton::Touch10Released => "Touch 10",
    HexButton::Touch11 | HexButton::Touch11Released => "Touch 11",
    HexButton::Touch12 | HexButton::Touch12Released => "Touch 12",
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

fn render_state(pressed: u32) -> LcdScreen {
  let lines: Vec<MenuLine> = ALL_BUTTONS
    .iter()
    .filter(|btn| pressed & bit(btn) == 0)
    .map(|btn| MenuLine(Icon20::Info, button_label(btn).to_string()))
    .collect();

  let remaining = lines.len();

  let _msg = if remaining == 0 {
    "All buttons work!".to_string()
  } else {
    format!("{remaining} buttons remaining")
  };
  LcdScreen::Menu {
    menu: lines,
    selected: 0,
    animation: MenuAnimation::None,
  }
}

impl<P: Platform> InputTestApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self { ctx, pressed: 0 }
  }
}

impl<P: Platform> MenuApp<P> for InputTestApp<P> {
  fn render(&self) -> LcdScreen {
    render_state(self.pressed)
  }

  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction {
    loop {
      let event = ctx.next().await;
      if let Some(action) = event.exit_action() {
        return action;
      }
      let AppRunEvent::Input(AppInput::Button(btn)) = event else {
        continue;
      };
      self.pressed |= bit(&btn);
      self.ctx.update_lcd(self.render());
      let total = ALL_BUTTONS.len();
      if self.pressed.count_ones() as usize >= total {
        return AppAction::Stop;
      }
    }
  }
}
