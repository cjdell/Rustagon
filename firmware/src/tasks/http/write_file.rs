use crate::{
  lib::{HttpSender, HttpStatusMessage},
  tasks::http::common::json_response,
  utils::local_fs::LocalFs,
};
use alloc::{format, vec::Vec};
use esp_alloc::ExternalMemory;
use esp_println::print;
use esp_storage::FlashStorage;
use log::info;
use picoserve::{io::Read, response::IntoResponse};
use serde::Serialize;

pub struct WriteFileHandler {
  local_fs: LocalFs,
  sender: HttpSender,
}

impl WriteFileHandler {
  pub fn new(local_fs: LocalFs, sender: HttpSender) -> Self {
    Self { local_fs, sender }
  }
}

impl picoserve::routing::RequestHandlerService<()> for WriteFileHandler {
  async fn call_request_handler_service<R: Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    mut request: picoserve::request::Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let query = request.parts.query().unwrap().try_into_string::<50>().unwrap();

    let file_name = query.replace("file=", "");
    let file_size = request.body_connection.content_length();

    info!("Write file: {}", file_name);

    // let mut flash = FlashStorage::new(unsafe { Peripherals::steal() }.FLASH); // Allow safe write whilst 2nd core enabled

    // let local_fs = match LocalFs::new(&mut flash) {
    //   Ok(fs) => fs,
    //   Err(err) => {
    //     return format!("LocalFs Init Error: {err:?}")
    //       .write_to(request.body_connection.finalize().await?, response_writer)
    //       .await;
    //   }
    // };

    let mut reader = request.body_connection.body().reader();

    let mut buffer = Vec::new_in(ExternalMemory);
    buffer.resize(FlashStorage::SECTOR_SIZE as usize, 0u8);

    let mut written_bytes: usize = 0;

    loop {
      let mut chunk_bytes = 0usize;

      // Make sure the buffer is full
      loop {
        let read_bytes = reader.read(&mut buffer[chunk_bytes..]).await?;
        chunk_bytes += read_bytes as usize;
        if read_bytes == 0 {
          break;
        }
      }

      if chunk_bytes == 0 {
        self.sender.send(HttpStatusMessage::None).await;
        return format!("Expecting more data: file_size={file_size} written_bytes={written_bytes}")
          .write_to(request.body_connection.finalize().await?, response_writer)
          .await;
      }

      let last_chunk = file_size <= written_bytes + chunk_bytes;

      if let Err(err) =
        self.local_fs.write_binary_chunk(&file_name, written_bytes as u64, &buffer[0..chunk_bytes], last_chunk)
      {
        self.sender.send(HttpStatusMessage::None).await;
        return format!("Write Error: {err:?}")
          .write_to(request.body_connection.finalize().await?, response_writer)
          .await;
      }

      self.sender.send(HttpStatusMessage::Progress(written_bytes as u32, file_size as u32)).await;
      print!(".");

      written_bytes += chunk_bytes;

      if last_chunk {
        break;
      }
    }

    let connection = request.body_connection.finalize().await?;
    self.sender.send(HttpStatusMessage::None).await;

    #[derive(Serialize)]
    struct ResponseJson {
      pub written_bytes: usize,
    }

    json_response(&serde_json::to_string(&ResponseJson { written_bytes }).unwrap())
      .write_to(connection, response_writer)
      .await
  }
}
