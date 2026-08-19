use crate::{
  alloc_ext::external_box,
  apps::{AppAction, AppError, AppEvent, AppInput, AppRunContext, AppRunEvent, MenuApp, MenuAppContext, common::AppName},
  platform::{Platform, TcpEvent, TcpSession},
  ssh::{
    PlatformRng, SshEvent, SshSession,
    terminal::{DISPLAY_LINES, Terminal, hex_button_to_bytes, key_to_bytes},
  },
  types::*,
  utils::select_timeout,
};
use alloc::{
  boxed::Box,
  format,
  string::{String, ToString},
  vec,
  vec::Vec,
};
use log::{debug, error, info};
use puressh::key::PrivateKey;

/// Max time to wait for the TCP connection to establish.
const CONNECT_TIMEOUT_MS: u64 = 15_000;
/// Max time to wait for any single handshake packet before giving up.
const HANDSHAKE_TIMEOUT_MS: u64 = 15_000;

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
  tcp_session: Option<TcpSession>,
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
      tcp_session: None,
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
    if let Some(session) = self.tcp_session.take() {
      session.close().await;
    }
    self.session = None;
    self.screen = Screen::Connect;
    self.ctx.update_lcd(self.render());
  }

  /// Flush every queued outbound SSH frame to the TCP connection.
  async fn flush(tcp: &TcpSession, session: &mut SshSession) {
    while let Some(frame) = session.poll_transmit() {
      tcp.send(frame).await;
    }
  }

  /// Send `bytes` to the remote shell (as SSH channel data).
  async fn send_bytes(&mut self, bytes: Vec<u8>) {
    let Some(tcp) = self.tcp_session.as_ref() else { return };
    let platform = self.ctx.platform.clone();
    let mut rng = PlatformRng { platform: &platform };
    let Some(session) = self.session.as_mut() else { return };
    if session.send_data(&bytes, &mut rng).is_ok() {
      Self::flush(tcp, session).await;
    }
  }

  /// Establish the TCP connection, perform the SSH handshake, authenticate and
  /// open the interactive shell.
  ///
  /// Cancellable: at every await point, any input/event (notably the boot
  /// button) aborts the handshake with `Err`. The caller (`run`) surfaces the
  /// error via `fail()` and the half-open TCP connection is closed by
  /// `on_stop` when the app is popped.
  async fn connect(&mut self, ctx: &AppRunContext<'_, P>) -> Result<(), AppError> {
    self.screen = Screen::Connecting;
    self.status = "Connecting...".to_string();
    self.ctx.update_lcd(self.render());

    let Some(tcp) = self.ctx.platform.tcp_client() else {
      return Err(AppError::Unsupported("No TCP support on this platform".into()));
    };
    let host = self.fields[0].value.trim().to_string();
    let user = self.fields[1].value.trim().to_string();
    let key_path = self.fields[2].value.trim().to_string();
    let port: u16 = self.fields[3].value.trim().parse().unwrap_or(22);

    if host.is_empty() || user.is_empty() {
      return Err(AppError::Message("Host and user required".into()));
    }

    let connected = embassy_futures::select::select(select_timeout(tcp.connect(host, port), CONNECT_TIMEOUT_MS), ctx.next()).await;
    let tcp_session = match connected {
      embassy_futures::select::Either::First(Some(Ok(session))) => session,
      embassy_futures::select::Either::First(Some(Err(()))) => return Err(AppError::Network),
      embassy_futures::select::Either::First(None) => return Err(AppError::Timeout),
      embassy_futures::select::Either::Second(_) => return Err(AppError::Message("Cancelled".into())),
    };

    self.status = "Loading key...".to_string();
    self.ctx.update_lcd(self.render());

    let key_pem = match embassy_futures::select::select(self.ctx.platform.storage_manager().read_text_file(key_path), ctx.next()).await {
      embassy_futures::select::Either::First(Ok(pem)) => pem,
      embassy_futures::select::Either::First(Err(_)) => return Err(AppError::NotFound("Key not found".into())),
      embassy_futures::select::Either::Second(_) => return Err(AppError::Message("Cancelled".into())),
    };
    let private_key = match PrivateKey::parse_openssh_pem(&key_pem, None) {
      Ok(k) => k,
      Err(err) => return Err(AppError::Message(format!("Bad key: {err:?}"))),
    };
    if private_key.algorithm() != "ssh-ed25519" {
      return Err(AppError::Message("Only ed25519 keys are supported".into()));
    }
    let host_key = match private_key.into_host_key() {
      Ok(h) => h,
      Err(err) => return Err(AppError::Message(format!("Key unusable: {err:?}"))),
    };

    self.status = "Handshake...".to_string();
    self.ctx.update_lcd(self.render());
    info!("SshApp: starting session handshake");

    let platform = self.ctx.platform.clone();
    let mut rng = PlatformRng { platform: &platform };
    let mut session = external_box(SshSession::new(user, host_key, &mut rng));
    if let Err(err) = session.start(&mut rng) {
      return Err(AppError::Message(format!("Handshake failed: {err:?}")));
    }
    info!("SshApp: session started, flushing {} outbound frames", session.outbox_len());
    Self::flush(&tcp_session, &mut session).await;
    debug!("SshApp: flushed initial frames");

    // Pump the handshake + auth until the shell is open, the session fails,
    // or input arrives (cancels the handshake mid-flight).
    loop {
      let event = match embassy_futures::select::select(select_timeout(tcp_session.next_event(), HANDSHAKE_TIMEOUT_MS), ctx.next()).await {
        embassy_futures::select::Either::First(Some(ev)) => ev,
        embassy_futures::select::Either::First(None) => return Err(AppError::Timeout),
        embassy_futures::select::Either::Second(_) => return Err(AppError::Message("Cancelled".into())),
      };
      debug!("SshApp: handshake event: {event:?}");
      match event {
        TcpEvent::Data(bytes) => {
          debug!("SshApp: feeding {} bytes to session", bytes.len());
          if let Err(err) = session.handle_input(&bytes, &mut rng) {
            return Err(AppError::Message(format!("Protocol error: {err:?}")));
          }
          Self::flush(&tcp_session, &mut session).await;
          while let Some(ev) = session.poll_event() {
            debug!("SshApp: session event: {ev:?}");
            match ev {
              SshEvent::Ready => {
                self.session = Some(session);
                self.tcp_session = Some(tcp_session);
                self.screen = Screen::Terminal;
                self.status = String::new();
                self.terminal = Terminal::new();
                self.ctx.update_lcd(self.terminal.render());
                return Ok(());
              }
              SshEvent::AuthFailed => return Err(AppError::Message("Authentication failed".into())),
              SshEvent::Closed => return Err(AppError::Message("Connection closed".into())),
              SshEvent::Error(msg) => return Err(AppError::Message(msg)),
              _ => {}
            }
          }
        }
        TcpEvent::Closed | TcpEvent::Error => return Err(AppError::Network),
      }
    }
  }

  /// Feed one inbound TCP event (plus everything else pending) into the SSH
  /// engine, render terminal output, and flush any outbound frames.
  /// Disconnects if the connection closed.
  async fn pump_session(&mut self, first: TcpEvent) {
    let platform = self.ctx.platform.clone();

    let mut closed = matches!(first, TcpEvent::Closed | TcpEvent::Error);
    let mut inbound: Vec<Vec<u8>> = Vec::new();
    if let TcpEvent::Data(data) = first {
      inbound.push(data);
    }
    {
      let Some(session) = self.tcp_session.as_ref() else { return };
      while let Some(ev) = session.try_next_event() {
        match ev {
          TcpEvent::Data(data) => inbound.push(data),
          TcpEvent::Closed | TcpEvent::Error => closed = true,
        }
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
    {
      let Some(session) = self.tcp_session.as_ref() else { return };
      for frame in frames {
        session.send(frame).await;
      }
    }
    if closed {
      self.disconnect().await;
    } else {
      self.ctx.update_lcd(self.terminal.render());
    }
  }

  /// Track shift state from both press and release (applied to characters
  /// typed while it is held).
  fn track_shift(&mut self, ke: &KeyboardEvent) {
    if ke.code != KeyCode::Shift {
      return;
    }
    self.shifted = ke.typ == KeyEventType::Pressed;
  }

  /// Handle a keyboard event on the connect screen (field editing).
  fn handle_connect_key(&mut self, ke: &KeyboardEvent) {
    self.track_shift(ke);
    if ke.typ == KeyEventType::Released || ke.code == KeyCode::Shift {
      return;
    }
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
    }
  }

  /// Handle a keyboard event on the terminal screen (send key bytes to the
  /// remote shell).
  async fn handle_terminal_key(&mut self, ke: &KeyboardEvent) {
    self.track_shift(ke);
    if ke.typ != KeyEventType::Pressed || ke.code == KeyCode::Shift {
      return;
    }
    if let Some(bytes) = key_to_bytes(ke.code, self.shifted) {
      self.send_bytes(bytes).await;
    }
  }
}

impl<P: Platform> MenuApp<P> for SshApp<P> {
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

  async fn run(&mut self, ctx: AppRunContext<'_, P>) -> AppAction {
    self.ctx.update_lcd(self.render());

    loop {
      match self.screen {
        Screen::Connect => {
          let event = ctx.next().await;
          if let Some(action) = event.exit_action() {
            return action;
          }
          match event {
            AppRunEvent::Input(AppInput::Button(hex)) => {
              match hex {
                HexButton::Up if self.active > 0 => self.active -= 1,
                HexButton::Down if self.active < self.field_count() - 1 => self.active += 1,
                HexButton::Fire if self.active == self.fields.len() => {
                  if let Err(err) = self.connect(&ctx).await {
                    self.fail(err.to_display());
                  }
                }
                _ => {}
              }
              self.ctx.update_lcd(self.render());
            }
            AppRunEvent::Event(AppEvent::Device(DeviceEvent::Keyboard(ke))) => {
              self.handle_connect_key(&ke);
              self.ctx.update_lcd(self.render());
            }
            _ => {}
          }
        }
        Screen::Connecting => {
          // Only reachable transiently (connect() runs inside the Connect arm
          // and cancels itself on input): wait for any event and re-check.
          let event = ctx.next().await;
          if let Some(action) = event.exit_action() {
            return action;
          }
        }
        Screen::Terminal => {
          let Some(tcp) = self.tcp_session.as_ref() else {
            // Defensive: the session is gone — back to the connect form.
            self.screen = Screen::Connect;
            continue;
          };
          let event = embassy_futures::select::select(ctx.next(), tcp.next_event()).await;
          match event {
            embassy_futures::select::Either::First(run_event) => {
              if let Some(action) = run_event.exit_action() {
                return action;
              }
              match run_event {
                AppRunEvent::Input(AppInput::Button(hex)) => {
                  if let Some(bytes) = hex_button_to_bytes(hex) {
                    self.send_bytes(bytes).await;
                  }
                }
                AppRunEvent::Event(AppEvent::Device(DeviceEvent::Keyboard(ke))) => {
                  self.handle_terminal_key(&ke).await;
                }
                _ => {}
              }
            }
            embassy_futures::select::Either::Second(tcp_event) => {
              // Inbound shell data renders immediately — no input or tick
              // cadence required.
              self.pump_session(tcp_event).await;
            }
          }
        }
      }
    }
  }

  /// Always close the socket on pop, not just when `Stop` arrives — the boot
  /// button pops us without a Stop, and without this the TCP pump leaks.
  async fn on_stop(&mut self) {
    self.disconnect().await;
  }
}
