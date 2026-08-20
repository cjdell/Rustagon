//! A tiny confirm dialog: a title + message and a Yes/No choice. Pushed by a
//! parent app with `AppAction::Push("Confirm", AppParams::Confirm { .. })`;
//! returns `AppAction::Result(AppResult::Confirm(bool))` on Fire and
//! `Stop` (→ `Cancelled`) on boot/back.

use crate::{
  apps::{common::AppName, AppAction, AppInput, AppParams, AppResult, AppRunContext, AppRunEvent, MenuApp, MenuAppContext},
  platform::Platform,
  types::*,
};
use alloc::string::{String, ToString};
use alloc::vec;

pub struct ConfirmationApp<P: Platform> {
  ctx: MenuAppContext<P>,
  title: String,
  message: String,
  /// Which of the two choice rows is selected (index 0 = Yes, 1 = No).
  choice_yes: bool,
}

impl<P: Platform> AppName for ConfirmationApp<P> {
  fn app_name() -> &'static str {
    "Confirm"
  }
}

impl<P: Platform> ConfirmationApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self::with_params(ctx, AppParams::None)
  }

  /// Construct from a push payload; anything but `AppParams::Confirm` gets a
  /// generic "Proceed?" prompt.
  pub fn with_params(ctx: MenuAppContext<P>, params: AppParams) -> Self {
    let (title, message) = match params {
      AppParams::Confirm { title, message } => (title, message),
      _ => ("Confirm".to_string(), "Proceed?".to_string()),
    };
    Self {
      ctx,
      title,
      message,
      choice_yes: true,
    }
  }
}

impl<P: Platform> MenuApp<P> for ConfirmationApp<P> {
  fn render(&self) -> LcdScreen {
    let choice_row = 2 + usize::from(!self.choice_yes);
    LcdScreen::Menu {
      menu: vec![
        MenuLine(Icon20::Info, self.title.clone()),
        MenuLine(Icon20::Info, self.message.clone()),
        MenuLine(Icon20::Info, "[Yes]".to_string()),
        MenuLine(Icon20::Info, "[No]".to_string()),
      ],
      selected: choice_row as u32,
      animation: MenuAnimation::FromRight,
    }
  }

  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction {
    self.ctx.update_lcd(self.render());

    loop {
      let event = ctx.next().await;
      if let Some(action) = event.exit_action() {
        return action; // boot → Stop → Cancelled for the parent
      }
      let AppRunEvent::Input(AppInput::Button(hex)) = event else {
        continue;
      };
      let action = match hex {
        HexButton::Up | HexButton::Down => {
          self.choice_yes = !self.choice_yes;
          AppAction::Continue
        }
        HexButton::Fire => return AppAction::Result(AppResult::Confirm(self.choice_yes)),
        HexButton::Left | HexButton::Right => return AppAction::Stop,
        _ => AppAction::Continue,
      };
      self.ctx.update_lcd(self.render());
      match action {
        AppAction::Continue => {}
        other => return other,
      }
    }
  }
}
