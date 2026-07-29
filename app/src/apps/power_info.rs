use crate::{
  apps::{common::AppName, MenuAppAsync, MenuAppInput, MenuAppContext},
  platform::Platform,
  types::*,
  utils::sleep,
};
use alloc::{
  format,
  string::{String, ToString},
  vec,
  vec::Vec,
};
use embassy_futures::select::{select, Either};

pub struct PowerInfoApp<P: Platform> {
  ctx: MenuAppContext<P>,
}

impl<P: Platform> AppName for PowerInfoApp<P> {
  fn app_name() -> &'static str {
    "Power Info"
  }
}

impl<P: Platform> PowerInfoApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    Self { ctx }
  }
}

impl<P: Platform> MenuAppAsync for PowerInfoApp<P> {
  async fn work(&mut self) -> bool {
    loop {
      let status = self.ctx.platform.power_manager().get_status().await;

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

      self.ctx.update_lcd(LcdScreen::Menu { menu: lines, selected: 0 });

      // Refresh every 2 seconds, or exit on Fire / Stop
      match select(
        self.ctx.input_receiver.receive(),
        sleep(2_000),
      )
      .await
      {
        Either::First(MenuAppInput::Stop) | Either::First(MenuAppInput::HexButton(HexButton::Fire)) => return false,
        _ => {}
      }
    }
  }
}
