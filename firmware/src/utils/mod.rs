pub mod bq25895;
pub mod cpu_guard;
pub mod dns;
pub mod flash_stream;
pub mod gpio;
pub mod graphics;
pub mod http;
pub mod i2c;
pub mod led_service;
pub mod local_fs;
pub mod ota;
pub mod spi;
pub mod state;

pub use gpio::Aw9523bGpioPin;
pub use i2c::{MaskedI2cBus, MultiplexedI2cBus};

use alloc::alloc::Global;
use alloc::vec::Vec;
use embassy_time::{Duration, Timer};
use embedded_graphics::{pixelcolor::Rgb565, prelude::IntoStorage};
use esp_alloc::{ExternalMemory, MemoryCapability};
use esp_hal::time::Instant;
use esp_println::{print, println};

pub fn print_memory_info() {
  for region in esp_alloc::HEAP.stats().region_stats {
    if let Some(region_info) = region {
      let cap = if region_info.capabilities.contains(MemoryCapability::Internal) {
        "Internal"
      } else {
        "External"
      };

      println!("{} {}/{}", cap, region_info.used, region_info.size);
    }
  }
}

pub fn print_memory(ptr: *const u8, len: usize) {
  unsafe {
    println!("Reading from address: 0x{:08x}", ptr as usize);

    // Read first 64 bytes
    for i in 0..len {
      let byte = ptr.add(i).read_volatile();
      print!("{:02x} ", byte);
      if (i + 1) % 16 == 0 {
        println!();
      }
    }
    println!();
  }
}

pub struct VecHelper;

impl VecHelper {
  pub fn do_vecs_match<T: PartialEq>(a: &Vec<T>, b: &Vec<T>) -> bool {
    let matching = a.iter().zip(b.iter()).filter(|&(a, b)| a == b).count();
    matching == a.len() && matching == b.len()
  }

  pub fn new_external_buffer(size: usize) -> Vec<u8, ExternalMemory> {
    let mut buffer = Vec::new_in(ExternalMemory);
    buffer.resize(size, 0u8);
    buffer
  }

  /// No copy. Treat a Vec allocated in PSRAM (ExternalMemory) as a normal Vec
  /// Both allocators are static global so this is safe
  pub fn to_global_vec<T>(vec: Vec<T, ExternalMemory>) -> Vec<T, Global> {
    let (ptr, len, cap, _) = vec.into_raw_parts_with_alloc();
    unsafe { Vec::<T>::from_parts(core::ptr::NonNull::new_unchecked(ptr), len, cap) }
  }
}

pub async fn sleep(ms: u64) {
  Timer::after(Duration::from_millis(ms)).await;
}

pub fn now() -> u32 {
  Instant::now().duration_since_epoch().as_millis() as u32
}

pub trait ConvertBE16 {
  fn to_be16(&self) -> u16;
}

impl ConvertBE16 for Rgb565 {
  fn to_be16(&self) -> u16 {
    IntoStorage::into_storage(*self).to_be()
  }
}
