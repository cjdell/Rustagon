use crate::utils::local_fs::LocalFs;
use alloc::format;
use embedded_io_async::Read;
use esp_hal::{
  peripherals::CPU_CTRL,
  system::{Cpu, CpuControl},
};
use picoserve::response::IntoResponse;

pub struct DeleteFileHandler {
  local_fs: LocalFs,
}

impl DeleteFileHandler {
  pub(crate) fn new(local_fs: LocalFs) -> Self {
    Self { local_fs }
  }
}

impl picoserve::routing::RequestHandlerService<()> for DeleteFileHandler {
  async fn call_request_handler_service<R: Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    request: picoserve::request::Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    let query = request.parts.query().unwrap().try_into_string::<50>().unwrap();

    let file_name = query.replace("file=", "");

    let mut cpu_ctrl = CpuControl::new(unsafe { CPU_CTRL::steal() });
    unsafe { cpu_ctrl.park_core(Cpu::AppCpu) };

    if let Err(err) = self.local_fs.delete_file(&file_name) {
      cpu_ctrl.unpark_core(Cpu::AppCpu);

      return format!("Delete Error: {err:?}")
        .write_to(request.body_connection.finalize().await?, response_writer)
        .await;
    }

    cpu_ctrl.unpark_core(Cpu::AppCpu);

    format!("Deleted: {file_name}\r\n").write_to(request.body_connection.finalize().await?, response_writer).await
  }
}
