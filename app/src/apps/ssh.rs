use crate::{
  alloc_ext::external_box,
  apps::{AppAction, AppEvent, MenuApp, MenuAppContext, MenuAppInput, common::AppName},
  platform::{Platform, TcpEvent, TcpEventChannel, TcpHandle},
  ssh::{
    PlatformRng, SshEvent, SshSession,
    terminal::{DISPLAY_LINES, Terminal, hex_button_to_bytes, key_to_bytes},
  },
  types::*,
  utils::sleep,
};
use alloc::{
  boxed::Box,
  format,
  string::{String, ToString},
  vec,
  vec::Vec,
};
use embassy_futures::select::{Either, select};
use log::{error, info};
use puressh::key::PrivateKey;

/// A connect-screen input field.
struct Field {
  label: &'static str,
  value: String,
}

impl Field {
  fn new(label: &'static str, value: &str) -> Self {
    Self {
      label,
      value: value.to_string(),
    }
  }
}

#[derive(PartialEq)]
enum Screen {
  Connect,
  Connecting,
  Terminal,
}

/// An SSH client for the badge: connect to a host, authenticate with an
/// ed25519 private key, and drive an interactive shell on the 8-line display.
///
/// The session is driven by a [`SshSession`] (a no_std puressh state machine)
/// fed by a platform TCP pump. See the `ssh` module docs for the engine.
pub struct SshApp<P: Platform> {
  ctx: MenuAppContext<P>,
  screen: Screen,
  fields: Vec<Field>,
  /// Index into `fields`, or `fields.len()` for the "[Connect]" action.
  active: usize,
  status: String,
  shifted: bool,
  /// The session lives on the heap (external memory on firmware) so the big
  /// SSH state machine never occupies the stack or inflates the menu stack
  /// enum.
  session: Option<Box<SshSession>>,
  tcp: Option<TcpHandle>,
  channel: Option<&'static TcpEventChannel>,
  terminal: Terminal,
}

impl<P: Platform> AppName for SshApp<P> {
  fn app_name() -> &'static str {
    "SSH"
  }
}

impl<P: Platform> SshApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    let fields = vec![
      Field::new("host", "192.168.49.1"),
      Field::new("user", "cjdell"),
      Field::new("key", "id_ed255.key"),
      Field::new("port", "22"),
    ];
    Self {
      ctx,
      screen: Screen::Connect,
      fields,
      active: 0,
      status: String::new(),
      shifted: false,
      session: None,
      tcp: None,
      channel: None,
      terminal: Terminal::new(),
    }
  }

  fn field_count(&self) -> usize {
    self.fields.len() + 1
  }

  /// Show an error and return to the connect screen.
  fn fail(&mut self, msg: impl Into<String>) {
    let msg = msg.into();
    info!("SshApp: {msg}");
    self.status = msg;
    self.screen = Screen::Connect;
    self.session = None;
    self.ctx.update_lcd(self.render());
  }

  /// Close the TCP connection and return to the connect screen.
  async fn disconnect(&mut self) {
    if let Some(tcp) = self.tcp.take() {
      tcp.close().await;
    }
    self.session = None;
    self.channel = None;
    self.screen = Screen::Connect;
    self.ctx.update_lcd(self.render());
  }

  /// Flush every queued outbound SSH frame to the TCP connection.
  async fn flush(tcp: &TcpHandle, session: &mut SshSession) {
    while let Some(frame) = session.poll_transmit() {
      tcp.send(frame).await;
    }
  }

  /// Send `bytes` to the remote shell (as SSH channel data).
  async fn send_bytes(&mut self, bytes: Vec<u8>) {
    let Some(tcp) = self.ctx.platform.tcp_client() else { return };
    let platform = self.ctx.platform.clone();
    let mut rng = PlatformRng { platform: &platform };
    let Some(session) = self.session.as_mut() else { return };
    if session.send_data(&bytes, &mut rng).is_ok() {
      Self::flush(&tcp, session).await;
    }
  }

  /// Establish the TCP connection, perform the SSH handshake, authenticate and
  /// open the interactive shell.
  async fn connect(&mut self) {
    self.screen = Screen::Connecting;
    self.status = "Connecting...".to_string();
    self.ctx.update_lcd(self.render());

    let Some(tcp) = self.ctx.platform.tcp_client() else {
      self.fail("No TCP support on this platform");
      return;
    };
    let host = self.fields[0].value.trim().to_string();
    let user = self.fields[1].value.trim().to_string();
    let key_path = self.fields[2].value.trim().to_string();
    let port: u16 = self.fields[3].value.trim().parse().unwrap_or(22);

    if host.is_empty() || user.is_empty() {
      self.fail("Host and user required");
      return;
    }

    // The background TCP pump needs a `'static` channel; leak one per session.
    // Bounded (16 events), and recycled when the session's connection closes.
    let channel: &'static TcpEventChannel = Box::leak(Box::new(TcpEventChannel::new()));
    tcp.connect(host, port, channel).await;

    // Wait for the connection to establish (or fail).
    let mut outcome = None;
    while outcome.is_none() {
      match select(channel.receive(), sleep(15_000)).await {
        Either::First(TcpEvent::Connected) => outcome = Some(Ok(())),
        Either::First(TcpEvent::Error | TcpEvent::Closed) => outcome = Some(Err(())),
        // Any other event (e.g. stray data) before Connected: keep waiting.
        Either::First(_) => {}
        Either::Second(_) => {
          self.fail("Connection timed out");
          return;
        }
      }
    }
    if outcome.unwrap().is_err() {
      self.fail("Connection failed");
      return;
    }

    self.status = "Loading key...".to_string();
    self.ctx.update_lcd(self.render());

    let key_pem = match self.ctx.platform.storage_manager().read_text_file(key_path).await {
      Ok(pem) => pem,
      Err(_) => {
        self.fail("Key not found");
        return;
      }
    };
    let private_key = match PrivateKey::parse_openssh_pem(&key_pem, None) {
      Ok(k) => k,
      Err(err) => {
        self.fail(format!("Bad key: {err:?}"));
        return;
      }
    };
    if private_key.algorithm() != "ssh-ed25519" {
      self.fail("Only ed25519 keys are supported");
      return;
    }
    let host_key = match private_key.into_host_key() {
      Ok(h) => h,
      Err(err) => {
        self.fail(format!("Key unusable: {err:?}"));
        return;
      }
    };

    self.status = "Handshake...".to_string();
    self.ctx.update_lcd(self.render());
    info!("SshApp: starting session handshake");

    let platform = self.ctx.platform.clone();
    let mut rng = PlatformRng { platform: &platform };
    let mut session = external_box(SshSession::new(user, host_key, &mut rng));
    if let Err(err) = session.start(&mut rng) {
      self.fail(format!("Handshake failed: {err:?}"));
      return;
    }
    info!("SshApp: session started, flushing {} outbound frames", session.outbox_len());
    Self::flush(&tcp, &mut session).await;
    info!("SshApp: flushed initial frames");

    // Pump the handshake + auth until the shell is open or the session fails.
    loop {
      let result = select(channel.receive(), sleep(15_000)).await;
      let event = match result {
        Either::First(ev) => ev,
        Either::Second(_) => {
          self.fail("Handshake timed out");
          return;
        }
      };
      info!("SshApp: handshake event: {event:?}");
      match event {
        TcpEvent::Data(bytes) => {
          info!("SshApp: feeding {} bytes to session", bytes.len());
          if let Err(err) = session.handle_input(&bytes, &mut rng) {
            self.fail(format!("Protocol error: {err:?}"));
            return;
          }
          Self::flush(&tcp, &mut session).await;
          while let Some(ev) = session.poll_event() {
            info!("SshApp: session event: {ev:?}");
            match ev {
              SshEvent::Ready => {
                self.session = Some(session);
                self.tcp = Some(tcp);
                self.channel = Some(channel);
                self.screen = Screen::Terminal;
                self.status = String::new();
                self.terminal = Terminal::new();
                self.ctx.update_lcd(self.terminal.render());
                return;
              }
              SshEvent::AuthFailed => {
                self.fail("Authentication failed");
                return;
              }
              SshEvent::Closed => {
                self.fail("Connection closed");
                return;
              }
              SshEvent::Error(msg) => {
                self.fail(msg);
                return;
              }
              _ => {}
            }
          }
        }
        TcpEvent::Closed | TcpEvent::Error => {
          self.fail("Connection lost");
          return;
        }
        _ => {}
      }
    }
  }

  /// Pump the session while the terminal is foregrounded: drain inbound TCP
  /// data into the SSH engine, feed output to the terminal, and flush any
  /// outbound frames.
  async fn pump_session(&mut self) {
    let Some(channel) = self.channel else { return };
    let Some(tcp) = self.ctx.platform.tcp_client() else { return };
    let platform = self.ctx.platform.clone();

    let mut closed = false;
    let mut inbound: Vec<Vec<u8>> = Vec::new();
    while let Ok(ev) = channel.try_receive() {
      match ev {
        TcpEvent::Data(data) => inbound.push(data),
        TcpEvent::Closed | TcpEvent::Error => closed = true,
        _ => {}
      }
    }
    if closed {
      info!("SshApp: connection closed");
      self.disconnect().await;
      return;
    }
    if inbound.is_empty() {
      return;
    }

    let mut rng = PlatformRng { platform: &platform };
    let mut frames: Vec<Vec<u8>> = Vec::new();
    {
      let session = match self.session.as_mut() {
        Some(s) => s,
        None => return,
      };
      for bytes in inbound {
        if let Err(err) = session.handle_input(&bytes, &mut rng) {
          error!("SshApp: protocol error: {err:?}");
          closed = true;
          break;
        }
        while let Some(frame) = session.poll_transmit() {
          frames.push(frame);
        }
        while let Some(ev) = session.poll_event() {
          match ev {
            SshEvent::Data(data) => self.terminal.feed(&data),
            SshEvent::Closed => closed = true,
            _ => {}
          }
        }
      }
    }
    for frame in frames {
      tcp.send(frame).await;
    }
    if closed {
      self.disconnect().await;
    } else {
      self.ctx.update_lcd(self.terminal.render());
    }
  }
}

impl<P: Platform> MenuApp for SshApp<P> {
  fn render(&self) -> LcdScreen {
    match self.screen {
      Screen::Connect => {
        let mut lines = vec![TextBufferLine {
          text: "SSH Client".into(),
          cursor: None,
        }];
        for (i, f) in self.fields.iter().enumerate() {
          lines.push(TextBufferLine {
            text: format!("{}: {}", f.label, f.value),
            cursor: (i == self.active).then_some(f.value.len() as u32),
          });
        }
        lines.push(TextBufferLine {
          text: "[Connect]".into(),
          cursor: (self.active == self.fields.len()).then_some(0),
        });
        lines.push(TextBufferLine {
          text: self.status.clone(),
          cursor: None,
        });
        while lines.len() < DISPLAY_LINES {
          lines.push(TextBufferLine {
            text: String::new(),
            cursor: None,
          });
        }
        LcdScreen::TextBuffer { lines }
      }
      Screen::Connecting => LcdScreen::Headline(Icon40::Info, self.status.clone()),
      Screen::Terminal => self.terminal.render(),
    }
  }

  async fn init(&mut self) {
    self.ctx.update_lcd(self.render());
  }

  async fn handle_input(&mut self, input: MenuAppInput) -> AppAction {
    match self.screen {
      Screen::Connect => match input {
        MenuAppInput::Stop => AppAction::Stop,
        MenuAppInput::Button(hex) => match hex {
          HexButton::Up => {
            if self.active > 0 {
              self.active -= 1;
            }
            AppAction::Continue
          }
          HexButton::Down => {
            if self.active < self.field_count() - 1 {
              self.active += 1;
            }
            AppAction::Continue
          }
          HexButton::Fire if self.active == self.fields.len() => {
            self.connect().await;
            AppAction::Continue
          }
          _ => AppAction::Continue,
        },
      },
      Screen::Connecting => {
        // Ignore input while the handshake is in flight.
        AppAction::Continue
      }
      Screen::Terminal => {
        if let MenuAppInput::Stop = input {
          self.disconnect().await;
          return AppAction::Stop;
        }
        if let MenuAppInput::Button(hex) = input {
          if let Some(bytes) = hex_button_to_bytes(hex) {
            self.send_bytes(bytes).await;
          }
        }
        self.pump_session().await;
        AppAction::Continue
      }
    }
  }

  async fn handle_event(&mut self, event: AppEvent) {
    let AppEvent::Device(DeviceEvent::Keyboard(ke)) = event else {
      return;
    };
    if ke.typ == KeyEventType::Released {
      if ke.code == KeyCode::Shift {
        self.shifted = false;
      }
      return;
    }
    if ke.code == KeyCode::Shift {
      self.shifted = true;
      return;
    }

    match self.screen {
      Screen::Connect => {
        let active = self.active;
        if active < self.fields.len() {
          match ke.code {
            KeyCode::Backspace => {
              self.fields[active].value.pop();
            }
            _ => {
              if let Some(ch) = ke.code.to_char(self.shifted) {
                self.fields[active].value.push(ch);
              }
            }
          }
          self.ctx.update_lcd(self.render());
        }
      }
      Screen::Terminal => {
        if let Some(bytes) = key_to_bytes(ke.code, self.shifted) {
          self.send_bytes(bytes).await;
        }
        self.pump_session().await;
      }
      Screen::Connecting => {}
    }
  }
}
