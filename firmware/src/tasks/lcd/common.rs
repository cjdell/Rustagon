use crate::{
  lib::{Icon20, Icon40, Image},
  utils::graphics::BufferTarget,
};
use embedded_graphics::prelude::{Point, Size};
use partitions_macro::include_rgb565_icon;

#[unsafe(link_section = ".rodata.mydata")]
pub static RUST_LOGO: &[u8] = include_rgb565_icon!("assets/images/rust.png");

#[unsafe(link_section = ".rodata.mydata")]
static HOME_20: &[u8] = include_rgb565_icon!("assets/icons/20x20/home.png");
#[unsafe(link_section = ".rodata.mydata")]
static CONFIG_20: &[u8] = include_rgb565_icon!("assets/icons/20x20/config.png");
#[unsafe(link_section = ".rodata.mydata")]
static WIFI_20: &[u8] = include_rgb565_icon!("assets/icons/20x20/wifi.png");
#[unsafe(link_section = ".rodata.mydata")]
static FILE_20: &[u8] = include_rgb565_icon!("assets/icons/20x20/file.png");
#[unsafe(link_section = ".rodata.mydata")]
static INFO_20: &[u8] = include_rgb565_icon!("assets/icons/20x20/info.png");

#[unsafe(link_section = ".rodata.mydata")]
static INFO_40: &[u8] = include_rgb565_icon!("assets/icons/40x40/info.png");
#[unsafe(link_section = ".rodata.mydata")]
static WARN_40: &[u8] = include_rgb565_icon!("assets/icons/40x40/warn.png");
#[unsafe(link_section = ".rodata.mydata")]
static ERROR_40: &[u8] = include_rgb565_icon!("assets/icons/40x40/error.png");
#[unsafe(link_section = ".rodata.mydata")]
static WIFI_40: &[u8] = include_rgb565_icon!("assets/icons/40x40/wifi.png");

pub trait Icon {
  fn size(&self) -> Size;
  fn data(&self) -> &[u8];
}

impl Icon for Image {
  fn size(&self) -> Size {
    Size::new(240, 240)
  }

  fn data(&self) -> &[u8] {
    match &self {
      Image::RustLogo => RUST_LOGO,
    }
  }
}

impl Icon for Icon20 {
  fn size(&self) -> Size {
    Size::new(20, 20)
  }

  fn data(&self) -> &[u8] {
    match &self {
      Icon20::Home => HOME_20,
      Icon20::Config => CONFIG_20,
      Icon20::Wifi => WIFI_20,
      Icon20::File => FILE_20,
      Icon20::Info => INFO_20,
    }
  }
}

impl Icon for Icon40 {
  fn size(&self) -> Size {
    Size::new(40, 40)
  }

  fn data(&self) -> &[u8] {
    match &self {
      Icon40::Info => INFO_40,
      Icon40::Warn => WARN_40,
      Icon40::Error => ERROR_40,
      Icon40::Wifi => WIFI_40,
    }
  }
}

pub fn draw_icon(display: &mut BufferTarget, pos: Point, icon: impl Icon) {
  let size = icon.size();
  let data = icon.data();

  display.draw_raw_image(pos.x, pos.y, size.width, size.height, data);
}
