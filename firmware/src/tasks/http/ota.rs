use crate::utils::ota::Ota;
use alloc::format;
use esp_hal::peripherals::FLASH;
use esp_println::print;
use esp_storage::FlashStorage;
use log::info;
use partitions_macro::partition_offset;
use picoserve::{
  ResponseSent,
  io::Read,
  request::Request,
  response::{IntoResponse, ResponseWriter},
  routing::RequestHandlerService,
};

const OTA_0_OFFSET: u32 = partition_offset!("ota_0");
const OTA_1_OFFSET: u32 = partition_offset!("ota_1");
const OTA_OFFSETS: [u32; 2] = [OTA_0_OFFSET, OTA_1_OFFSET];

pub struct OtaUpdateHandler;

impl RequestHandlerService<()> for OtaUpdateHandler {
  async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    mut request: Request<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    let mut storage = FlashStorage::new(unsafe { FLASH::steal() });
    let mut ota = Ota::new(&mut storage);

    let current_slot = ota.current_slot();
    info!("Current Slot: {:?}", current_slot);
    let new_slot = current_slot.next();
    info!("New Slot: {:?}", new_slot);

    let mut flash_addr = OTA_OFFSETS[new_slot.number()];

    let mut reader = request.body_connection.body().reader();

    let mut buffer = [0; FlashStorage::SECTOR_SIZE as usize];
    let mut total_size = 0;

    loop {
      let mut read_size = 0;

      // Make sure the buffer is full
      loop {
        let chunk_read_bytes = reader.read(&mut buffer[read_size..]).await?;
        read_size += chunk_read_bytes;
        if chunk_read_bytes == 0 {
          break;
        }
      }

      if read_size == 0 {
        break;
      }

      ota.write(flash_addr, &buffer[..read_size]).unwrap();
      flash_addr += read_size as u32;

      print!(".");

      total_size += read_size;
    }

    ota.set_current_slot(new_slot);

    format!("Total Size: {total_size}\r\n")
      .write_to(request.body_connection.finalize().await?, response_writer)
      .await
  }
}
