use crate::{
  apps::{common::AppName, AppAction, AppEvent, MenuApp, MenuAppInput, MenuAppContext},
  platform::Platform,
  types::*,
};
use alloc::{format, string::ToString, vec::Vec};
use log::info;

pub struct HexpansionViewerApp<P: Platform> {
  ctx: MenuAppContext<P>,
  slots: Vec<(u8, Option<HexpansionInfo>)>,
}

impl<P: Platform> AppName for HexpansionViewerApp<P> {
  fn app_name() -> &'static str {
    "Hexpansions"
  }
}

impl<P: Platform> HexpansionViewerApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self { ctx, slots: Vec::new() }
  }

  fn refresh_state(&mut self) {
    self.slots = self.ctx.platform.hexpansion_manager().current_state();
    info!("HexpansionViewerApp: refreshed state: {:#?}", self.slots);
  }
}

impl<P: Platform> MenuApp for HexpansionViewerApp<P> {
  fn render(&self) -> LcdScreen {
    let mut lines: Vec<MenuLine> = Vec::new();

    lines.push(MenuLine(Icon20::Info, "Hexpansions".to_string()));
    lines.push(MenuLine(Icon20::Info, "".to_string()));

    for (port, slot) in &self.slots {
      match slot {
        Some(info) => {
          lines.push(MenuLine(
            Icon20::Info,
            format!("Port {port}: {}", info.friendly_name),
          ));
          lines.push(MenuLine(
            Icon20::Info,
            format!("  {:04X}:{:04X}", info.vid, info.pid),
          ));
        }
        None => {
          lines.push(MenuLine(Icon20::Info, format!("Port {port}: empty")));
        }
      }
    }

    lines.push(MenuLine(Icon20::Info, "".to_string()));
    lines.push(MenuLine(Icon20::Info, "<= Back".to_string()));

    LcdScreen::Menu { menu: lines, selected: 0, animation: MenuAnimation::FromRight }
  }

  async fn init(&mut self) {
    // Retry a few times in case the polling task hasn't completed its first scan
    for _ in 0..3 {
      self.refresh_state();
      if self.slots.iter().any(|(_, s)| s.is_some()) {
        break;
      }
      crate::utils::sleep(500).await;
    }
    self.ctx.update_lcd(self.render());
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    self.refresh_state();
    match input {
      MenuAppInput::Stop => AppAction::Stop,
      MenuAppInput::Button(HexButton::Fire | HexButton::Left) => AppAction::Stop,
      _ => AppAction::Continue,
    }
  }

  async fn handle_event(&mut self, _event: AppEvent) {
    self.refresh_state();
  }
}
