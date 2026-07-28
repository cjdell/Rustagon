pub use app::platform::display::{DisplayError, DisplayHandle, DisplayManager};

use crate::{
  d_i2c::*,
  types::*,
  utils::{
    graphics::{BufferTarget, SCREEN_HEIGHT, SCREEN_WIDTH},
    spi::SpiExclusiveDevice,
    *,
  },
};
use alloc::vec::Vec;
use display_renderer::LcdState;
use aw9523b::Pin;
use core::fmt;
use core::ptr;
use core::slice::{from_raw_parts, from_raw_parts_mut};
use display_interface::{DataFormat, WriteOnlyDataCommand};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Delay;
use esp_alloc::ExternalMemory;
use esp_hal::{
  gpio::{Level, Output, OutputConfig},
  peripherals::Peripherals,
  spi::{Mode, master::{Config, Spi}},
  time::{Instant, Rate},
};
use gc9a01::{
  Gc9a01, SPIDisplayInterface,
  command::Command,
  mode::DisplayConfiguration,
  prelude::{DisplayResolution240x240, DisplayRotation, SPIInterface},
};
use log::info;

pub type LcdSignal = Signal<CriticalSectionRawMutex, LcdScreen>;

pub static mut BUFFER: *mut u8 = ptr::null_mut::<u8>();

pub static mut SPI_DISPLAY_INTERFACE: *mut SPIInterface<SpiExclusiveDevice<'_>, Output<'_>> =
  ptr::null_mut::<SPIInterface<SpiExclusiveDevice<'_>, Output<'_>>>();

pub struct HardwareDisplayManager {
  signal: &'static LcdSignal,
}

impl HardwareDisplayManager {
  pub fn new(signal: &'static LcdSignal) -> Self {
    Self { signal }
  }
}

impl fmt::Debug for HardwareDisplayManager {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("HardwareDisplayManager").finish()
  }
}

impl DisplayManager for HardwareDisplayManager {
  fn signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    self.signal.signal(screen);
    Ok(())
  }

  fn try_signal(&self, screen: LcdScreen) -> Result<(), DisplayError> {
    if self.signal.try_take().is_none() {
      self.signal.signal(screen);
    }
    Ok(())
  }

  fn frame_buffer(&self) -> Option<&[u8]> {
    unsafe {
      Some(from_raw_parts(
        BUFFER,
        (app::platform::display::DISPLAY_WIDTH * app::platform::display::DISPLAY_HEIGHT * 2) as usize,
      ))
    }
  }
}

#[embassy_executor::task]
pub async fn lcd_task(sys_bus: MaskedI2cBus, signal: &'static LcdSignal) {
  info!("Starting LCD Task...");
  info!("LCD: Initialising display");

  let p = unsafe { Peripherals::steal() };

  let mut reset = Aw9523bOutputPin::new(sys_bus, I2C_2, Pin::P16);

  let cs = Output::new(p.GPIO1, Level::High, OutputConfig::default());
  let dc = Output::new(p.GPIO2, Level::High, OutputConfig::default());

  let spi = Spi::new(
    p.SPI2,
    Config::default().with_frequency(Rate::from_mhz(80)).with_mode(Mode::_0),
  )
  .unwrap();

  let mut spi = spi.with_sck(p.GPIO8).with_mosi(p.GPIO7);

  let spi_device = SpiExclusiveDevice::new(&mut spi, cs);
  let mut interface = SPIDisplayInterface::new(spi_device, dc);

  let mut buffer = Vec::new_in(ExternalMemory);
  buffer.resize((SCREEN_WIDTH * SCREEN_HEIGHT * 2) as usize, 0u8);

  unsafe {
    BUFFER = buffer.as_mut_ptr();
    SPI_DISPLAY_INTERFACE = core::mem::transmute(&mut interface);
  }

  let mut display = Gc9a01::new(interface, DisplayResolution240x240, DisplayRotation::Rotate0);

  display.reset(&mut reset, &mut Delay).unwrap();
  display.init(&mut Delay).unwrap();
  display.clear().unwrap();

  let raw_buffer = unsafe { from_raw_parts_mut(BUFFER, (SCREEN_WIDTH * SCREEN_HEIGHT * 2) as usize) };
  let interface: &mut DisplayInterface = unsafe { core::mem::transmute(SPI_DISPLAY_INTERFACE) };

  let mut target = BufferTarget::new(buffer);

  let now = Instant::now().duration_since_epoch().as_millis() as i32;
  let mut state = LcdState::new(LcdScreen::Blank, now);

  'await_signal: loop {
    let now = Instant::now().duration_since_epoch().as_millis() as i32;
    state.update(signal.wait().await, now);

    loop {
      let now = Instant::now().duration_since_epoch().as_millis() as i32;
      if let Some(new_screen) = signal.try_take() {
        state.update(new_screen, now);
      }

      let now = Instant::now().duration_since_epoch().as_millis() as i32;
      state.notification_cleanup(now);

      if let LcdScreen::Blank = state.screen {
        continue 'await_signal;
      }

      loop {
        target.clear();

        let now = Instant::now().duration_since_epoch().as_millis() as i32;
        state.notification_cleanup(now);

        let next_frame = state.draw(&mut target, &state.screen, now);

        Command::ColumnAddressSet(0, SCREEN_WIDTH as u16 - 1).send(interface).ok();
        Command::RowAddressSet(0, SCREEN_HEIGHT as u16 - 1).send(interface).ok();
        Command::MemoryWrite.send(interface).ok();

        interface.send_data(DataFormat::U8(raw_buffer)).ok();

        if next_frame == 1_000 {
          continue 'await_signal;
        }
        if next_frame > 0 {
          sleep(next_frame as u64).await;
          break;
        }
      }
    }
  }
}
