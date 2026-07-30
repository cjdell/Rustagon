use crate::{
  apps::{common::AppName, AppAction, MenuApp, MenuAppInput, MenuAppContext},
  platform::Platform,
  types::*,
};
use alloc::{
  format,
  string::ToString,
  vec::Vec,
};

pub struct PowerInfoApp<P: Platform> {
  ctx: MenuAppContext<P>,
  last_screen: Option<LcdScreen>,
}

impl<P: Platform> AppName for PowerInfoApp<P> {
  fn app_name() -> &'static str {
    "Power Info"
  }
}

impl<P: Platform> PowerInfoApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self { ctx, last_screen: None }
  }
}

impl<P: Platform> MenuApp for PowerInfoApp<P> {
  fn render(&self) -> LcdScreen {
    self.last_screen.clone().unwrap_or(LcdScreen::Blank)
  }

  async fn init(&mut self) {
    let screen = build_status_screen(&self.ctx).await;
    self.last_screen = Some(screen.clone());
    self.ctx.update_lcd(screen);
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    match input {
      MenuAppInput::Stop => AppAction::Stop,
      MenuAppInput::Button(HexButton::Fire) => AppAction::Stop,
      MenuAppInput::Button(_) => AppAction::Continue,
    }
  }
}

async fn build_status_screen<P: Platform>(ctx: &MenuAppContext<P>) -> LcdScreen {
  let status = ctx.platform.power_manager().get_status().await;

  let mut lines: Vec<MenuLine> = Vec::new();

  lines.push(MenuLine(Icon20::Info, format!("Battery: {}%", status.battery_percent())));
  lines.push(MenuLine(Icon20::Info, format!("  {}.{:02}V", status.vbat_mv / 1000, (status.vbat_mv % 1000) / 10)));

  if status.is_power_present {
    lines.push(MenuLine(Icon20::Config, "USB powered".to_string()));
    lines.push(MenuLine(Icon20::Info, format!("VBUS: {}.{:02}V", status.vbus_mv / 1000, (status.vbus_mv % 1000) / 10)));
    lines.push(MenuLine(Icon20::Info, format!("Charge: {}mA", status.charge_current_ma)));
    lines.push(MenuLine(Icon20::Info, format!("Ilim: {}mA", status.input_current_limit_ma)));
  } else {
    lines.push(MenuLine(Icon20::Info, "USB: not connected".to_string()));
  }

  if status.battery_fault {
    lines.push(MenuLine(Icon20::Info, "Battery fault!".to_string()));
  }

  lines.push(MenuLine(Icon20::Info, format!("VSYS: {}.{:02}V", status.vsys_mv / 1000, (status.vsys_mv % 1000) / 10)));
  lines.push(MenuLine(Icon20::Info, format!("VREG: {}.{:02}V", status.charge_voltage_mv / 1000, (status.charge_voltage_mv % 1000) / 10)));
  lines.push(MenuLine(Icon20::Info, "<= Back".to_string()));

  LcdScreen::Menu { menu: lines, selected: 0 }
}
