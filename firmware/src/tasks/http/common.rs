use alloc::string::String;
use embedded_io_async::{Read, Write};
use picoserve::{
  ResponseSent,
  request::{Path, Request},
  response::{Connection, Content, File, IntoResponse, Response, ResponseWriter, StatusCode},
  routing::PathRouterService,
};

#[unsafe(link_section = ".rodata.mydata")]
#[used]
static HTML_DATA: &[u8] = include_bytes!("../../../../web/bundle/index.html.gz");

#[derive(Clone)]
pub struct AppState {}

pub struct StringResponse {
  pub str: String,
}

impl StringResponse {
  pub fn new(str: String) -> Self {
    Self { str }
  }
}

impl IntoResponse for StringResponse {
  async fn write_to<R: Read, W: ResponseWriter<Error = R::Error>>(
    self,
    connection: Connection<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    let Self { str } = self;

    str.write_to(connection, response_writer).await
  }
}

pub struct BinaryResponse<'a> {
  pub bin: &'a [u8],
}

impl<'a> IntoResponse for BinaryResponse<'a> {
  async fn write_to<R: Read, W: ResponseWriter<Error = R::Error>>(
    self,
    connection: Connection<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    let Self { bin } = self;

    bin.write_to(connection, response_writer).await
  }
}

// pub trait GetContentLength {
//   async fn get_content_length(&self) -> Result<usize, anyhow::Error>;
// }

// impl<'a> GetContentLength for RequestParts<'a> {
//   async fn get_content_length(&self) -> Result<usize, anyhow::Error> {
//     match self.headers().get("Content-Length") {
//       Some(contentLength) => Ok(contentLength.as_str()?.parse::<usize>()?),
//       None => Err(anyhow::anyhow!("No Content-Length field in request!")),
//     }
//   }
// }

macro_rules! format_response {
  ($request:expr, $response_writer:expr, $($arg:tt)*) => {
    format!($($arg)*)
      .write_to($request.body_connection.finalize().await?, $response_writer)
      .await
  };
}

macro_rules! json_response {
  ($request:expr, $response_writer:expr, $json:expr) => {
    json_response($json).write_to($request.body_connection.finalize().await?, $response_writer).await
  };
}

macro_rules! read_request_to_buffer {
  ($request:expr, $response_writer:expr) => {{
    let file_size = $request.body_connection.body().content_length();
    let mut buffer = Vec::new_in(ExternalMemory);
    buffer.resize(file_size, 0u8);
    let mut reader = $request.body_connection.body().reader();
    match reader.read_exact(&mut buffer).await {
      Ok(()) => Ok(()),
      Err(err) => match err {
        embedded_io::ReadExactError::UnexpectedEof => {
          return format_response!($request, $response_writer, "UnexpectedEof: Expected {file_size} bytes");
        }
        embedded_io::ReadExactError::Other(err) => Err(err),
      },
    }?;
    buffer
  }};
}

pub struct CustomNotFound;

impl PathRouterService<()> for CustomNotFound {
  async fn call_path_router_service<R: Read, W: ResponseWriter<Error = R::Error>>(
    &self,
    _state: &(),
    _path_parameters: (),
    path: Path<'_>,
    request: Request<'_, R>,
    response_writer: W,
  ) -> Result<ResponseSent, W::Error> {
    if request.parts.method() == "OPTIONS" {
      cors_options_response().write_to(request.body_connection.finalize().await?, response_writer).await
    } else {
      (
        StatusCode::NOT_FOUND,
        format_args!("Path \"{:?}\" not found!\n", path.encoded()),
      )
        .write_to(request.body_connection.finalize().await?, response_writer)
        .await
    }
  }
}

pub fn redirect_home_response() -> impl IntoResponse {
  Response::new(StatusCode::TEMPORARY_REDIRECT, "").with_headers([("Location", "/")])
}

pub fn html_app_response() -> impl IntoResponse {
  Response::new(StatusCode::OK, HtmlApp).with_headers([("Content-Encoding", "gzip")])
}

pub fn cors_options_response() -> impl IntoResponse {
  Response::new(StatusCode::OK, "").with_headers([
    ("Access-Control-Allow-Origin", "*"),
    ("Access-Control-Allow-Methods", "*"),
    ("Access-Control-Allow-Headers", "*"),
  ])
}

pub fn json_response(json: &str) -> impl IntoResponse {
  Response::new(StatusCode::OK, json).with_headers([
    ("Access-Control-Allow-Origin", "*"),
    ("Content-Type", "application/json"),
  ])
}

pub fn text_response(json: &str) -> impl IntoResponse {
  Response::new(StatusCode::OK, json)
    .with_headers([("Access-Control-Allow-Origin", "*"), ("Content-Type", "text/plain")])
}

pub struct HtmlApp;

impl Content for HtmlApp {
  fn content_type(&self) -> &'static str {
    File::MIME_HTML
  }

  fn content_length(&self) -> usize {
    HTML_DATA.len()
  }

  async fn write_content<W: Write>(self, writer: W) -> Result<(), W::Error> {
    HTML_DATA.write_content(writer).await
  }
}
