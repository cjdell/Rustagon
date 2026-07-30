use app::wasm::host::WasmHost;
use core::slice::from_raw_parts_mut;
use display_interface::{DataFormat, WriteOnlyDataCommand};
use esp_hal::{
    gpio::{AnyPin, Level, Output},
    time::Instant,
};
use esp_println::print;
use gc9a01::command::Command;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::platform::display::{BUFFER, SPI_DISPLAY_INTERFACE};
use crate::types::DisplayInterface;
use crate::utils::graphics::{SCREEN_HEIGHT, SCREEN_WIDTH};

pub struct HardwareWasmHost;

static LAST_SCREEN_UPDATE: AtomicU32 = AtomicU32::new(0);

impl WasmHost for HardwareWasmHost {
    fn write_stdout(&mut self, text: &str) {
        print!("{text}");
    }

    fn get_millis(&self) -> u64 {
        Instant::now().duration_since_epoch().as_millis()
    }

    fn set_lcd_buffer(&mut self, buffer: &[u8]) {
        let interface: &mut DisplayInterface = unsafe { core::mem::transmute(SPI_DISPLAY_INTERFACE) };

        Command::ColumnAddressSet(0, SCREEN_WIDTH as u16 - 1)
            .send(interface)
            .ok();
        Command::RowAddressSet(0, SCREEN_HEIGHT as u16 - 1)
            .send(interface)
            .ok();
        Command::MemoryWrite.send(interface).ok();
        interface.send_data(DataFormat::U8(buffer)).ok();

        let now = Instant::now().duration_since_epoch().as_millis() as u32;
        let last = LAST_SCREEN_UPDATE.load(Ordering::Relaxed);

        if now.wrapping_sub(last) > 250 {
            LAST_SCREEN_UPDATE.store(now, Ordering::Relaxed);

            let raw_buffer =
                unsafe { from_raw_parts_mut(BUFFER, (SCREEN_WIDTH * SCREEN_HEIGHT * 2) as usize) };
            raw_buffer.copy_from_slice(buffer);
        }
    }

    fn set_gpio(&mut self, pin_number: u32, state: u32) {
        let pin = unsafe { AnyPin::steal(pin_number.try_into().unwrap()) };
        let mut output = Output::new(pin, Level::High, Default::default());
        output.set_level(if state == 0 { Level::Low } else { Level::High });
    }
}
