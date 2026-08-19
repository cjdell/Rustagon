use super::sleep;
use crate::platform::Platform;
use picoserve::{
  io::Read,
  request::Request,
  response::{IntoResponse, ResponseWriter},
  routing::RequestHandlerService,
};

pub struct RebootHandler<P: Platform> {
  platform: P,
}

impl<P: Platform> RebootHandler<P> {
  pub fn new(platform: P) -> Self {
    Self { platform }
  }
}

impl<P: Platform> RequestHandlerService<()> for RebootHandler<P> {
  async fn call_request_handler_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    (): &(),
    (): (),
    request: Request<'_, R>,
    response_writer: W,
  ) -> Result<picoserve::ResponseSent, W::Error> {
    // Send the response *before* resetting — `software_reset` never returns,
    // so awaiting it first would leave the client waiting forever.
    alloc::string::String::from("OK")
      .write_to(request.body_connection.finalize().await?, response_writer)
      .await?;

    // Give the network stack time to flush the response before the reset.
    sleep(100).await;
    self.platform.software_reset().await;
    unreachable!()
  }
}
