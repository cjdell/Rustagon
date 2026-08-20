use crate::{
  alloc_ext::external_box,
  apps::{
    common::AppName, confirm::ConfirmationApp, files::FilesApp, AppAction, AppError, AppEvent, AppInput, AppParams, AppResult,
    AppRunContext, AppRunEvent, MenuApp, MenuAppContext,
  },
  platform::{Platform, TcpEvent, TcpSession},
  ssh::{
    keys::{hex_button_to_bytes, key_to_bytes},
    PlatformRng, SshEvent, SshSession,
  },
  types::*,
  ui::{
    form::{Field, Form},
    terminal::{Terminal, DISPLAY_LINES},
  },
  utils::select_timeout,
};
use alloc::{
  boxed::Box,
  format,
  string::{String, ToString},
  vec,
  vec::Vec,
};
use log::{debug, error, info, warn};
use puressh::key::PrivateKey;

/// Max time to wait for the TCP connection to establish.
const CONNECT_TIMEOUT_MS: u64 = 15_000;
/// Max time to wait for any single handshake packet before giving up.
const HANDSHAKE_TIMEOUT_MS: u64 = 15_000;

/// The SSH app's per-app KV namespace.
const KV_NAMESPACE: &str = "ssh";

/// Persisted connect-form settings (KV key `"connect"`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
struct SshSettings {
  host: String,
  user: String,
  key: String,
  port: u16,
}

/// Outcome of one pass through the handshake pump.
#[derive(Debug, PartialEq)]
enum PumpStep {
  /// The shell is open and the session is usable.
  Ready,
  /// The server host key's fingerprint differs from the stored one.
  /// Carries the stored fingerprint (for the confirm prompt); the caller
  /// must ask the user and then resume or abort.
  KeyChanged(String),
}

/// What a `connect()` attempt ended up as.
#[derive(Debug, PartialEq)]
enum ConnectOutcome {
  /// Connected, authenticated, shell open.
  Ready,
  /// Paused on a host-key change; the session is parked in
  /// `self.session`/`self.tcp_session` awaiting the confirm dialog's result.
  KeyChanged(String),
}

/// The SHA-256 fingerprint of a server host key in SSH wire format, as hex
/// (the same value `ssh-keygen -lf` prints, sans the key type and comment).
fn host_key_fingerprint(key: &[u8]) -> String {
  use purecrypto::hash::{Digest, Sha256};
  let digest = Sha256::digest(key);
  digest.iter().map(|b| format!("{b:02x}")).collect()
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
  /// The connect form: host/user/key/port fields + the "[Connect]" action row.
  form: Form,
  status: String,
  shifted: bool,
  /// The session lives on the heap (external memory on firmware) so the big
  /// SSH state machine never occupies the stack or inflates the menu stack
  /// enum.
  session: Option<Box<SshSession>>,
  tcp_session: Option<TcpSession>,
  terminal: Terminal,
  /// Set once the saved settings have been loaded into the form.
  kv_loaded: bool,
  /// `Some(stored fingerprint)` while a host-key-change confirmation is
  /// pending (the handshake is parked in `session`/`tcp_session`).
  key_confirm: Option<String>,
  /// Host/port of the current or last connection attempt (keys the
  /// per-host host-key fingerprints in the KV store).
  host: String,
  port: u16,
}

impl<P: Platform> AppName for SshApp<P> {
  fn app_name() -> &'static str {
    "SSH"
  }
}

impl<P: Platform> SshApp<P> {
  pub fn new(ctx: MenuAppContext<P>) -> Self {
    let form = Form::new(
      vec![
        Field::new("host", "192.168.49.1"),
        Field::new("user", "cjdell"),
        Field::new("key", "id_ed255.key"),
        Field::new("port", "22"),
      ],
      "[Connect]",
    );
    Self {
      ctx,
      screen: Screen::Connect,
      form,
      status: String::new(),
      shifted: false,
      session: None,
      tcp_session: None,
      terminal: Terminal::new(),
      kv_loaded: false,
      key_confirm: None,
      host: String::new(),
      port: 22,
    }
  }

  /// The value of connect-form field `index`.
  fn field_value(&self, index: usize) -> &str {
    self.form.field(index).expect("field index in range").value()
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
    self.key_confirm = None;
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

  /// Establish the TCP connection, perform the SSH handshake (with the
  /// trust-on-first-use host-key check), authenticate and open the
  /// interactive shell.
  ///
  /// Cancellable: at every await point, any input/event (notably the boot
  /// button) aborts the handshake with `Err`. The caller (`run`) surfaces the
  /// error via `fail()` and the half-open TCP connection is closed by
  /// `on_stop` when the app is popped.
  ///
  /// `Ok(KeyChanged(stored))` means the server presented a different host
  /// key than the one stored for this host: the half-open session is parked
  /// in `self.session`/`self.tcp_session` and the caller must push the
  /// confirm dialog, then either `resume_handshake` (user said Yes, the new
  /// key becomes the stored one) or `disconnect` (user said No).
  async fn connect(&mut self, ctx: &AppRunContext<'_, P>) -> Result<ConnectOutcome, AppError> {
    self.screen = Screen::Connecting;
    self.status = "Connecting...".to_string();
    self.ctx.update_lcd(self.render());

    let Some(tcp) = self.ctx.platform.tcp_client() else {
      return Err(AppError::Unsupported("No TCP support on this platform".into()));
    };
    self.host = self.field_value(0).trim().to_string();
    let user = self.field_value(1).trim().to_string();
    let key_path = self.field_value(2).trim().to_string();
    self.port = self.field_value(3).trim().parse().unwrap_or(22);

    if self.host.is_empty() || user.is_empty() {
      return Err(AppError::Message("Host and user required".into()));
    }

    // Remember what the user is trying to connect to (survives relaunch).
    self.save_settings().await;

    let connected = embassy_futures::select::select(
      select_timeout(tcp.connect(self.host.clone(), self.port), CONNECT_TIMEOUT_MS),
      ctx.next_interrupt(),
    )
    .await;
    let tcp_session = match connected {
      embassy_futures::select::Either::First(Some(Ok(session))) => session,
      embassy_futures::select::Either::First(Some(Err(()))) => return Err(AppError::Network),
      embassy_futures::select::Either::First(None) => return Err(AppError::Timeout),
      embassy_futures::select::Either::Second(_) => return Err(AppError::Message("Cancelled".into())),
    };

    self.status = "Loading key...".to_string();
    self.ctx.update_lcd(self.render());

    let key_pem =
      match embassy_futures::select::select(self.ctx.platform.storage_manager().read_text_file(key_path), ctx.next_interrupt()).await {
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
    info!("SshApp: starting session handshake to {user}@{}:{}", self.host, self.port);

    let platform = self.ctx.platform.clone();
    let mut rng = PlatformRng { platform: &platform };
    let mut session = external_box(SshSession::new(user, host_key, &mut rng));
    if let Err(err) = session.start(&mut rng) {
      return Err(AppError::Message(format!("Handshake failed: {err:?}")));
    }
    info!("SshApp: session started, {} outbound frames", session.outbox_len());

    match Self::pump_handshake(&self.ctx, self.host.as_str(), self.port, ctx, &mut session, &tcp_session).await? {
      PumpStep::Ready => {
        self.session = Some(session);
        self.tcp_session = Some(tcp_session);
        self.screen = Screen::Terminal;
        self.status = String::new();
        self.terminal = Terminal::new();
        self.ctx.update_lcd(self.terminal.render());
        Ok(ConnectOutcome::Ready)
      }
      PumpStep::KeyChanged(stored) => {
        // Park the half-open session; the confirm dialog decides.
        self.session = Some(session);
        self.tcp_session = Some(tcp_session);
        self.key_confirm = Some(stored.clone());
        self.status = "Host key changed".to_string();
        self.ctx.update_lcd(self.render());
        Ok(ConnectOutcome::KeyChanged(stored))
      }
    }
  }

  /// Pump a (possibly resumed) handshake + auth until the shell is open
  /// (`Ready`), the host-key check demands confirmation (`KeyChanged`), the
  /// session fails, or input arrives (cancels with `Err`).
  ///
  /// An associated function (not `&mut self`): the caller may already hold
  /// disjoint borrows of `self.session`/`self.tcp_session` (the resume path),
  /// so this takes exactly the pieces it needs.
  async fn pump_handshake(
    app_ctx: &MenuAppContext<P>,
    host: &str,
    port: u16,
    ctx: &AppRunContext<'_, P>,
    session: &mut SshSession,
    tcp: &TcpSession,
  ) -> Result<PumpStep, AppError> {
    let mut rng = PlatformRng {
      platform: &app_ctx.platform,
    };
    Self::flush(tcp, session).await;
    debug!("SshApp: flushed initial frames");

    loop {
      let event = match embassy_futures::select::select(select_timeout(tcp.next_event(), HANDSHAKE_TIMEOUT_MS), ctx.next_interrupt()).await
      {
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
          Self::flush(tcp, session).await;
          while let Some(ev) = session.poll_event() {
            debug!("SshApp: session event: {ev:?}");
            match ev {
              // First KEX reply seen: trust-on-first-use host-key check.
              SshEvent::Connected => {
                if let Some(step) = Self::check_host_key(app_ctx, host, port, session.server_host_key()).await {
                  return Ok(step);
                }
              }
              SshEvent::Ready => return Ok(PumpStep::Ready),
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

  /// Trust-on-first-use host-key decision (runs once, on
  /// `SshEvent::Connected`, when the first KEX reply has been seen). Takes
  /// the server host key in SSH wire format (from
  /// [`SshSession::server_host_key`]; `None` before the first KEX reply).
  /// First contact stores the fingerprint and trusts it; a matching stored
  /// fingerprint passes; a changed fingerprint returns `KeyChanged(stored)`
  /// so the caller can ask the user.
  async fn check_host_key(app_ctx: &MenuAppContext<P>, host: &str, port: u16, server_key: Option<&[u8]>) -> Option<PumpStep> {
    let key = server_key?;
    let fingerprint = host_key_fingerprint(key);
    let key_id = format!("host_key:{host}:{port}");
    let kv = app_ctx.kv(KV_NAMESPACE);
    let stored = kv.get::<String>(&key_id).await.ok().flatten();
    match stored {
      None => {
        info!("SshApp: new host {key_id}, trusting fingerprint {fingerprint} (TOFU)");
        if let Err(err) = kv.set(&key_id, fingerprint).await {
          warn!("SshApp: saving host-key fingerprint failed: {err}");
        }
        None
      }
      Some(stored) if stored == fingerprint => None,
      Some(stored) => {
        warn!("SshApp: host key for {key_id} changed (stored {stored}, now {fingerprint})");
        Some(PumpStep::KeyChanged(stored))
      }
    }
  }

  /// Resume a handshake parked on a host-key change (the user confirmed the
  /// new key). On success the new fingerprint becomes the stored one.
  async fn resume_handshake(&mut self, ctx: &AppRunContext<'_, P>) -> Result<(), AppError> {
    self.status = "Handshake...".to_string();
    self.ctx.update_lcd(self.render());

    let Some(session) = self.session.as_mut() else {
      return Err(AppError::Message("No session to resume".into()));
    };
    let Some(tcp) = self.tcp_session.as_ref() else {
      return Err(AppError::Message("No connection to resume".into()));
    };
    match Self::pump_handshake(&self.ctx, self.host.as_str(), self.port, ctx, session, tcp).await? {
      PumpStep::Ready => {
        // The user accepted the changed key: make it the stored one.
        if let Some(key) = session.server_host_key() {
          let fingerprint = host_key_fingerprint(key);
          let key_id = format!("host_key:{}:{}", self.host, self.port);
          info!("SshApp: user accepted changed host key for {key_id} ({fingerprint})");
          if let Err(err) = self.ctx.kv(KV_NAMESPACE).set(&key_id, fingerprint).await {
            warn!("SshApp: saving updated host-key fingerprint failed: {err}");
          }
        }
        self.key_confirm = None;
        self.screen = Screen::Terminal;
        self.status = String::new();
        self.terminal = Terminal::new();
        self.ctx.update_lcd(self.terminal.render());
        Ok(())
      }
      PumpStep::KeyChanged(stored) => Err(AppError::Message(format!("Host key changed again ({stored})"))),
    }
  }

  /// Fill the connect form from the KV store (first `run()` entry).
  async fn load_settings(&mut self) {
    let kv = self.ctx.kv(KV_NAMESPACE);
    match kv.get::<SshSettings>("connect").await {
      Ok(Some(settings)) => {
        info!(
          "SshApp: restored saved settings ({}@{}:{})",
          settings.user, settings.host, settings.port
        );
        self.set_field(0, &settings.host);
        self.set_field(1, &settings.user);
        self.set_field(2, &settings.key);
        self.set_field(3, &settings.port.to_string());
      }
      Ok(None) => {}
      Err(err) => warn!("SshApp: loading settings failed: {err}"),
    }
  }

  /// Persist the connect form (called on each connect attempt).
  async fn save_settings(&mut self) {
    let settings = SshSettings {
      host: self.field_value(0).trim().to_string(),
      user: self.field_value(1).trim().to_string(),
      key: self.field_value(2).trim().to_string(),
      port: self.field_value(3).trim().parse().unwrap_or(22),
    };
    if let Err(err) = self.ctx.kv(KV_NAMESPACE).set("connect", settings).await {
      warn!("SshApp: saving settings failed: {err}");
    }
  }

  /// Set connect-form field `index` (e.g. from a file-picker result).
  fn set_field(&mut self, index: usize, value: &str) {
    if let Some(field) = self.form.field_mut(index) {
      field.set_value(value);
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
    let Some(field) = self.form.active_field_mut() else {
      return; // active row is the [Connect] action — not editable
    };
    match ke.code {
      KeyCode::Backspace => field.input.backspace(),
      _ => {
        if let Some(ch) = ke.code.to_char(self.shifted) {
          field.input.push_char(ch);
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
        lines.extend(self.form.field_lines());
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
    // Refresh on show; the first entry also restores the saved connect form
    // from the KV store (host/user/key/port survive relaunch).
    if !self.kv_loaded {
      self.kv_loaded = true;
      self.load_settings().await;
    }
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
                HexButton::Up => self.form.up(),
                HexButton::Down => self.form.down(),
                HexButton::Fire if self.form.active_is_action() => match self.connect(&ctx).await {
                  Ok(ConnectOutcome::Ready) => {}
                  Ok(ConnectOutcome::KeyChanged(stored)) => {
                    info!("SshApp: host key changed (stored {stored}); asking the user");
                    return AppAction::Push(
                      ConfirmationApp::<P>::app_name().to_string(),
                      AppParams::Confirm {
                        title: "Host key changed".to_string(),
                        message: format!("{}@{}:{}", self.field_value(1), self.host, self.port),
                      },
                    );
                  }
                  Err(err) => self.fail(err.to_display()),
                },
                // Fire on the key field opens the file picker for the key path.
                HexButton::Fire if matches!(self.form.active_field().map(|f| f.label()), Some("key")) => {
                  return AppAction::Push(
                    FilesApp::<P>::app_name().to_string(),
                    AppParams::PickFile {
                      message: "Choose an ed25519 key file".to_string(),
                    },
                  );
                }
                _ => {}
              }
              self.ctx.update_lcd(self.render());
            }
            AppRunEvent::Event(AppEvent::Device(DeviceEvent::Keyboard(ke))) => {
              self.handle_connect_key(&ke);
              self.ctx.update_lcd(self.render());
            }
            AppRunEvent::Result(result) => {
              match result {
                AppResult::Path(path) => {
                  // The file picker chose a key.
                  self.set_field(2, &path);
                  self.status = String::new();
                }
                AppResult::Confirm(true) if self.key_confirm.take().is_some() => {
                  if let Err(err) = self.resume_handshake(&ctx).await {
                    self.fail(err.to_display());
                  }
                }
                AppResult::Confirm(false) | AppResult::Cancelled if self.key_confirm.take().is_some() => {
                  // The user declined (or dismissed) the changed key.
                  self.disconnect().await;
                  self.fail("Host key changed; refusing to connect");
                }
                _ => {}
              }
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

// ---------------------------------------------------------------------------
// Trust-on-first-use host-key decision (unit tests)
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "testing"))]
mod tofu_tests {
  use super::{host_key_fingerprint, PumpStep, SshApp};
  use crate::apps::MenuAppContext;
  use crate::protocol::HostIpcChannel;
  use crate::testing::MockPlatform;
  use alloc::boxed::Box;
  use alloc::string::String;
  use futures::executor::block_on;

  fn ctx(mock: &MockPlatform) -> MenuAppContext<MockPlatform> {
    let channel: &'static HostIpcChannel = Box::leak(Box::new(HostIpcChannel::new()));
    MenuAppContext::new(mock.clone(), channel.sender())
  }

  #[test]
  fn fingerprint_is_sha256_hex() {
    let fp = host_key_fingerprint(b"some-host-key-bytes");
    assert_eq!(fp.len(), 64, "SHA-256 hex is 64 chars: {fp}");
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
  }

  /// Trust-on-first-use: a new key is stored and trusted, a matching key
  /// passes, a changed key returns `KeyChanged(stored)` for the confirm
  /// dialog, and each host:port has its own entry.
  #[test]
  fn host_key_tofu_store_match_change() {
    let mock = MockPlatform::new();
    let ctx = ctx(&mock);
    let key_a = b"wire-format-host-key-A";
    let key_b = b"wire-format-host-key-B";
    let fp_a = host_key_fingerprint(key_a);

    block_on(async {
      // First contact: trusted and stored.
      assert_eq!(SshApp::check_host_key(&ctx, "1.2.3.4", 22, Some(key_a)).await, None);
      let stored = ctx
        .kv("ssh")
        .get::<String>("host_key:1.2.3.4:22")
        .await
        .unwrap()
        .expect("stored on first use");
      assert_eq!(stored, fp_a);

      // Same key again: passes silently.
      assert_eq!(SshApp::check_host_key(&ctx, "1.2.3.4", 22, Some(key_a)).await, None);

      // Changed key: KeyChanged carrying the stored fingerprint.
      assert_eq!(
        SshApp::check_host_key(&ctx, "1.2.3.4", 22, Some(key_b)).await,
        Some(PumpStep::KeyChanged(fp_a.clone()))
      );

      // A different host:port has its own (empty) entry.
      assert_eq!(SshApp::check_host_key(&ctx, "5.6.7.8", 22, Some(key_b)).await, None);

      // No key seen yet (pre-KEX): no decision.
      assert_eq!(SshApp::check_host_key(&ctx, "1.2.3.4", 22, None).await, None);
    });
  }
}
