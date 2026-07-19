#![no_std]
#![no_main]
#![recursion_limit = "256"]

use aw9523b::{Dir, Pin};
use embassy_executor::Spawner;
use embedded_hal::i2c::I2c as _;
use esp_alloc::heap_allocator;
use esp_backtrace as _;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::{
  i2c::master::{BusTimeout, I2c},
  time::Rate,
  timer::timg::TimerGroup,
};
use firmware::d_i2c::*;
use firmware::utils::*;
use log::{error, info, warn};

extern crate alloc;
extern crate core;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
  esp_println::logger::init_logger_from_env();

  let config = esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max());
  let peripherals = esp_hal::init(config);

  heap_allocator!(#[esp_hal::ram(reclaimed)] size: 72 * 1024);
  heap_allocator!(size: 72 * 1024);

  let timg0 = TimerGroup::new(peripherals.TIMG0);
  let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
  esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

  let i2c = I2c::new(
    peripherals.I2C0,
    esp_hal::i2c::master::Config::default().with_frequency(Rate::from_khz(100)).with_timeout(BusTimeout::BusCycles(133_000)),
  )
  .unwrap()
  .with_sda(peripherals.GPIO45)
  .with_scl(peripherals.GPIO46);

  let multiplexed_i2c_bus = MultiplexedI2cBus::new(i2c);

  let sys_bus = multiplexed_i2c_bus.new_masked_i2c_bus(MultiplexedI2cBus::SYS_BUS);
  let top_bus = multiplexed_i2c_bus.new_masked_i2c_bus(MultiplexedI2cBus::TOP_BUS);

  reset_device(peripherals.GPIO9);

  scan_devices(sys_bus.clone());
  scan_devices(top_bus.clone());

  init_gpio(sys_bus.clone(), I2C_0).await;
  init_gpio(sys_bus.clone(), I2C_1).await;
  init_gpio(sys_bus.clone(), I2C_2).await;

  let mut gpio_i2c_0 = aw9523b::Aw9523b::new(sys_bus.clone(), I2C_0);
  let mut gpio_i2c_1 = aw9523b::Aw9523b::new(sys_bus.clone(), I2C_1);
  let mut gpio_i2c_2 = aw9523b::Aw9523b::new(sys_bus.clone(), I2C_2);

  gpio_i2c_0.set_io_direction(Pin::P00, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P01, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P02, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P03, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P04, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P05, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P06, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P07, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P10, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P11, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P12, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P13, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P14, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P15, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P16, Dir::INPUT).unwrap();
  gpio_i2c_0.set_io_direction(Pin::P17, Dir::INPUT).unwrap();

  gpio_i2c_1.set_io_direction(Pin::P00, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P01, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P02, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P03, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P04, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P05, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P06, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P07, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P10, Dir::INPUT).unwrap(); // HexC
  gpio_i2c_1.set_io_direction(Pin::P11, Dir::INPUT).unwrap(); // HexD
  gpio_i2c_1.set_io_direction(Pin::P12, Dir::INPUT).unwrap(); // HexE
  gpio_i2c_1.set_io_direction(Pin::P13, Dir::INPUT).unwrap(); // HexF
  gpio_i2c_1.set_io_direction(Pin::P14, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P15, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P16, Dir::INPUT).unwrap();
  gpio_i2c_1.set_io_direction(Pin::P17, Dir::INPUT).unwrap();

  gpio_i2c_2.set_io_direction(Pin::P00, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P01, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P02, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P03, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P04, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P05, Dir::INPUT).unwrap(); // A / Up
  gpio_i2c_2.set_io_direction(Pin::P06, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P07, Dir::INPUT).unwrap(); // B / Right
  gpio_i2c_2.set_io_direction(Pin::P10, Dir::INPUT).unwrap(); // HexA
  gpio_i2c_2.set_io_direction(Pin::P11, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P12, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P13, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P14, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P15, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P16, Dir::INPUT).unwrap();
  gpio_i2c_2.set_io_direction(Pin::P17, Dir::INPUT).unwrap();

  loop {
    let mut bus = sys_bus.clone();

    let mut result = [0u8; 1];

    bus
      .transaction(
        I2C_0,
        &mut [
          embedded_hal::i2c::Operation::Write(&mut [0x0u8]),
          embedded_hal::i2c::Operation::Read(&mut result),
        ],
      )
      .unwrap();

    info!("Result Bus 0 Port 0: {result:?}");

    bus
      .transaction(
        I2C_1,
        &mut [
          embedded_hal::i2c::Operation::Write(&mut [0x1u8]),
          embedded_hal::i2c::Operation::Read(&mut result),
        ],
      )
      .unwrap();

    info!("Result Bus 0 Port 1: {result:?}");

    bus
      .transaction(
        I2C_1,
        &mut [
          embedded_hal::i2c::Operation::Write(&mut [0x0u8]),
          embedded_hal::i2c::Operation::Read(&mut result),
        ],
      )
      .unwrap();

    info!("Result Bus 1 Port 0: {result:?}");

    bus
      .transaction(
        I2C_1,
        &mut [
          embedded_hal::i2c::Operation::Write(&mut [0x1u8]),
          embedded_hal::i2c::Operation::Read(&mut result),
        ],
      )
      .unwrap();

    info!("Result Bus 1 Port 1: {result:?}");

    bus
      .transaction(
        I2C_2,
        &mut [
          embedded_hal::i2c::Operation::Write(&mut [0x0u8]),
          embedded_hal::i2c::Operation::Read(&mut result),
        ],
      )
      .unwrap();

    info!("Result Bus 2 Port 0: {result:?}");

    bus
      .transaction(
        I2C_2,
        &mut [
          embedded_hal::i2c::Operation::Write(&mut [0x1u8]),
          embedded_hal::i2c::Operation::Read(&mut result),
        ],
      )
      .unwrap();

    info!("Result Bus 2 Port 1: {result:?}");

    // let a_pressed = gpio_i2c_2.pin_is_low(Pin::P06).unwrap_or_default();
    // let b_pressed = gpio_i2c_2.pin_is_low(Pin::P07).unwrap_or_default();
    // let c_pressed = gpio_i2c_1.pin_is_low(Pin::P00).unwrap_or_default();
    // let d_pressed = gpio_i2c_1.pin_is_low(Pin::P01).unwrap_or_default();
    // let e_pressed = gpio_i2c_1.pin_is_low(Pin::P02).unwrap_or_default();
    // let f_pressed = gpio_i2c_1.pin_is_low(Pin::P03).unwrap_or_default();

    // let hex_a_pressed = gpio_i2c_2.pin_is_low(Pin::P10).unwrap_or_default();
    // let hex_b_pressed = gpio_i2c_2.pin_is_low(Pin::P15).unwrap_or_default();
    // let hex_c_pressed = gpio_i2c_1.pin_is_low(Pin::P10).unwrap_or_default();
    // let hex_d_pressed = gpio_i2c_1.pin_is_low(Pin::P11).unwrap_or_default();
    // let hex_e_pressed = gpio_i2c_1.pin_is_low(Pin::P12).unwrap_or_default();
    // let hex_f_pressed = gpio_i2c_1.pin_is_low(Pin::P13).unwrap_or_default();

    // self.i2c.write_read(self.addr, &[register as u8], &mut val)
    //     .map_err(Error::I2C)
    //     .and(Ok(val[0]))

    // info!(
    //   "{a_pressed} {b_pressed} {c_pressed} {d_pressed} {e_pressed} {f_pressed} {hex_a_pressed} {hex_b_pressed} {hex_c_pressed} {hex_d_pressed} {hex_e_pressed} {hex_f_pressed}"
    // );

    embassy_time::Timer::after(embassy_time::Duration::from_millis(1_000)).await;
  }
}
