//! A compact no_std SSH client engine built on the `puressh` sans-IO protocol
//! layers.
//!
//! `puressh`'s high-level `ClientDriver` is gated behind its `std`-only
//! `client` feature, so on the firmware we drive the underlying sans-IO
//! primitives directly: `VersionExchange`, `PacketCodec`, `KexRunner`,
//! `ClientAuth` and `ConnectionState`. This module re-implements the (small)
//! handshake + auth + channel orchestration that `ClientDriver` performs, using
//! only `puressh`'s `no_std` + `alloc` surface.
//!
//! The engine is purely state-machined: it performs no I/O and keeps no clock.
//! The caller feeds inbound bytes via [`SshSession::handle_input`], drains
//! outbound frames via [`SshSession::poll_transmit`], reads high-level events
//! via [`SshSession::poll_event`], and supplies both a cryptographic RNG and
//! the current time on every call. [`PlatformRng`] adapts a platform's entropy
//! source to `purecrypto`'s RNG traits.
//!
//! Only 25519-family algorithms are advertised, keeping the firmware small:
//! `curve25519-sha256` key exchange, `ssh-ed25519` host keys and publickey
//! auth, and `chacha20-poly1305@openssh.com` AEAD encryption.

pub mod terminal;

#[cfg(test)]
mod tests;

use alloc::{boxed::Box, collections::VecDeque, format, string::String, vec::Vec};
use log::{debug, info};
use purecrypto::rng::{CryptoRng, CryptoRngCore, RngCore};
use puressh::{
  auth::{ClientAuth, ClientCredential, ClientStep},
  channel::{ChannelEvent, ChannelOpen, ChannelRequest, ConnectionState},
  error::{Error, Result},
  hostkey::{HostKey, HostKeyVerify, host_key_verify_by_name},
  transport::{
    KexAlgorithms, KexInit, PacketCodec,
    ext_info::SSH_MSG_EXT_INFO,
    rekey::is_kex_msg,
    runner::{KexRunner, Role},
    version::{LOCAL_VERSION, VersionExchange},
  },
};

/// `SSH_MSG_KEX_ECDH_REPLY` (RFC 5656) — the server's answer carrying its host
/// key and the exchange-hash signature. `puressh` keeps this constant private
/// to its KEX runner, so we define our own.
const SSH_MSG_KEX_ECDH_REPLY: u8 = 31;

/// Safety cap on unparsed inbound bytes before the packet codec drains them.
const MAX_INBOX_BYTES: usize = 64 * 1024;
/// A single peer banner/version line may not exceed 255 bytes (RFC 4253 §4.2).
const MAX_BANNER_LINE: usize = 255;
/// Tolerate a bounded number of pre-version banner lines, then bail.
const MAX_BANNER_LINES: usize = 100;
/// Tolerate a bounded total of banner bytes.
const MAX_BANNER_TOTAL_BYTES: usize = 32 * 1024;

/// Backing store for the session's inbound buffer. On the firmware
/// (`extern-alloc`) the growing receive buffer lives in external memory
/// (PSRAM) so it never competes with the small internal SRAM heap; elsewhere
/// it is the default heap.
#[cfg(feature = "extern-alloc")]
type ExtVec = Vec<u8, esp_alloc::ExternalMemory>;
#[cfg(not(feature = "extern-alloc"))]
type ExtVec = Vec<u8>;

/// A fresh empty inbound buffer, allocated in external memory where available.
fn new_ext_vec() -> ExtVec {
  #[cfg(feature = "extern-alloc")]
  {
    use esp_alloc::ExternalMemory;
    Vec::new_in(ExternalMemory)
  }
  #[cfg(not(feature = "extern-alloc"))]
  {
    Vec::new()
  }
}

/// 25519-only algorithm advertisement. See the module docs.
///
/// The trailing `kex-strict-c-v00@openssh.com` and `ext-info-c` names are
/// *signalling* markers, not real algorithms: they tell the peer we support
/// strict-kex (Terrapin / CVE-2023-48795) and RFC 8308 EXT_INFO. Modern OpenSSH
/// (10.x) advertises its own markers and some hardened configurations require
/// the client to answer in kind, so we include them.
const ALGORITHMS: KexAlgorithms<'static> = KexAlgorithms {
  kex: &[
    "curve25519-sha256",
    "curve25519-sha256@libssh.org",
    puressh::transport::kex::STRICT_KEX_CLIENT_MARKER,
    puressh::transport::ext_info::EXT_INFO_CLIENT_MARKER,
  ],
  server_host_key: &["ssh-ed25519"],
  ciphers_c2s: &["chacha20-poly1305@openssh.com", "aes128-ctr"],
  ciphers_s2c: &["chacha20-poly1305@openssh.com", "aes128-ctr"],
  macs_c2s: &["hmac-sha2-256-etm@openssh.com", "hmac-sha2-512-etm@openssh.com", "hmac-sha2-256"],
  macs_s2c: &["hmac-sha2-256-etm@openssh.com", "hmac-sha2-512-etm@openssh.com", "hmac-sha2-256"],
  comp_c2s: &["none"],
  comp_s2c: &["none"],
  lang_c2s: &[],
  lang_s2c: &[],
};

/// A platform's entropy source adapted to `purecrypto`'s RNG traits.
///
/// This is the only "I/O" the SSH engine touches — every session needs a
/// CSPRNG for KEX ephemerals and packet padding, and on a badge that must come
/// from the hardware TRNG, not a deterministic seed.
pub struct PlatformRng<'a, P> {
  pub platform: &'a P,
}

impl<P: crate::platform::Platform> RngCore for PlatformRng<'_, P> {
  fn fill_bytes(&mut self, dest: &mut [u8]) {
    self.platform.entropy(dest);
  }
}

impl<P: crate::platform::Platform> CryptoRng for PlatformRng<'_, P> {}

/// High-level events surfaced by [`SshSession::poll_event`].
#[derive(Debug, Clone)]
pub enum SshEvent {
  /// The transport handshake completed and the auth flow started (informational).
  Connected,
  /// The interactive shell channel is open — the session is usable.
  Ready,
  /// Data received on the shell channel (stdout/stderr merged).
  Data(Vec<u8>),
  /// The remote process exited with this status.
  ExitStatus(u32),
  /// The session ended (peer closed, error, or local close).
  Closed,
  /// Authentication failed.
  AuthFailed,
  /// A non-fatal error message for the UI to show.
  Error(String),
}

/// Where in the SSH protocol lifecycle the session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
  /// Waiting for the peer's `SSH-2.0-…` identification line.
  AwaitingVersion,
  /// First key exchange in flight.
  Kex,
  /// Handshake done; post-NEWKEYS payloads (userauth, then channels).
  PostKex,
}

/// Sans-I/O SSH client session driving the `puressh` transport/auth/channel
/// layers. See the module docs for the pumping contract.
pub struct SshSession {
  phase: Phase,
  codec: PacketCodec,
  runner: KexRunner,
  inbox: ExtVec,
  outbox: VecDeque<Vec<u8>>,
  events: VecDeque<SshEvent>,
  /// Application packets received while a re-key was in flight (RFC 4253 §7.3).
  deferred: VecDeque<Vec<u8>>,
  /// Peer's version string (without CR/LF).
  v_s: Vec<u8>,
  session_id: Vec<u8>,
  user: String,
  /// The private key used for publickey auth; consumed when auth starts.
  host_key: Option<Box<dyn HostKey>>,
  auth: Option<ClientAuth>,
  conn: ConnectionState,
  /// Local id of the session channel once opened.
  channel: Option<u32>,
  pty_sent: bool,
  shell_sent: bool,
  eof_sent: bool,
  close_sent: bool,
  remote_close: bool,
  banner_lines: usize,
  banner_total: usize,
}

impl SshSession {
  /// Build a fresh session. `host_key` is the client's private key used for
  /// publickey authentication; `user` is the login name.
  pub fn new<R: CryptoRngCore>(user: String, host_key: Box<dyn HostKey>, rng: &mut R) -> Self {
    let advert = build_kexinit(rng);
    Self {
      phase: Phase::AwaitingVersion,
      codec: PacketCodec::new(),
      runner: KexRunner::new(Role::Client, advert),
      inbox: new_ext_vec(),
      outbox: VecDeque::new(),
      events: VecDeque::new(),
      deferred: VecDeque::new(),
      v_s: Vec::new(),
      session_id: Vec::new(),
      user,
      host_key: Some(host_key),
      auth: None,
      conn: ConnectionState::new(),
      channel: None,
      pty_sent: false,
      shell_sent: false,
      eof_sent: false,
      close_sent: false,
      remote_close: false,
      banner_lines: 0,
      banner_total: 0,
    }
  }

  /// Emit the local version line and the initial KEXINIT. Call once before
  /// pumping.
  pub fn start<R: CryptoRngCore>(&mut self, rng: &mut R) -> Result<()> {
    self.outbox.push_back(VersionExchange::outgoing_bytes());
    let advert = build_kexinit(rng);
    self.runner = KexRunner::new(Role::Client, advert);
    let initial = self.runner.start(rng)?;
    for p in initial.outbound {
      self.enqueue_payload(&p, rng)?;
    }
    Ok(())
  }

  /// Feed inbound transport bytes. Routes as many packets as are available,
  /// enqueuing outbound frames and high-level events.
  pub fn handle_input<R: CryptoRngCore>(&mut self, bytes: &[u8], rng: &mut R) -> Result<()> {
    self.inbox.extend_from_slice(bytes);
    if self.inbox.len() > MAX_INBOX_BYTES {
      return Err(Error::Protocol("inbound buffer too large"));
    }

    if self.phase == Phase::AwaitingVersion && !self.scan_peer_version()? {
      return Ok(()); // need more bytes for a complete line
    }

    loop {
      match self.codec.decode(&self.inbox)? {
        Some((payload, consumed)) => {
          debug!(
            "ssh: decoded msg={} consumed={} inbox_left={}",
            payload.first().copied().unwrap_or(0),
            consumed,
            self.inbox.len() - consumed
          );
          self.inbox.drain(..consumed);
          self.route_packet(&payload, rng)?;
        }
        None => {
          debug!("ssh: no more decodable packets (inbox {} bytes)", self.inbox.len());
          return Ok(());
        }
      }
    }
  }

  /// Pop the next fully-encoded frame to write to the transport, if any.
  pub fn poll_transmit(&mut self) -> Option<Vec<u8>> {
    self.outbox.pop_front()
  }

  /// Number of outbound frames queued and waiting to be flushed.
  pub fn outbox_len(&self) -> usize {
    self.outbox.len()
  }

  /// Pop the next high-level [`SshEvent`], if any.
  pub fn poll_event(&mut self) -> Option<SshEvent> {
    self.events.pop_front()
  }

  /// Encode `payload` with the current keys and queue it for transmission.
  pub fn enqueue_payload<R: CryptoRngCore>(&mut self, payload: &[u8], rng: &mut R) -> Result<()> {
    let frame = self.codec.encode(payload, rng)?;
    self.outbox.push_back(frame);
    Ok(())
  }

  /// Send `data` to the remote shell. Ignored until the shell is open.
  pub fn send_data<R: CryptoRngCore>(&mut self, data: &[u8], rng: &mut R) -> Result<()> {
    if let Some(ch) = self.channel {
      let (payload, _) = self.conn.send_data(ch, data)?;
      self.enqueue_payload(&payload, rng)?;
    }
    Ok(())
  }

  /// Send EOF then close the session channel, if one is open.
  pub fn close<R: CryptoRngCore>(&mut self, rng: &mut R) -> Result<()> {
    if let Some(ch) = self.channel {
      if !self.eof_sent {
        if let Ok(payload) = self.conn.send_eof(ch) {
          self.enqueue_payload(&payload, rng)?;
        }
        self.eof_sent = true;
      }
      if !self.close_sent {
        if let Ok(payload) = self.conn.send_close(ch) {
          self.enqueue_payload(&payload, rng)?;
        }
        self.close_sent = true;
      }
    }
    Ok(())
  }

  /// True once the interactive shell is open and the session is usable.
  pub fn is_ready(&self) -> bool {
    self.channel.is_some() && self.shell_sent
  }

  // --- internal routing ---

  /// Consume version-exchange preamble + the `SSH-2.0-…` line from `inbox`.
  /// Returns `Ok(true)` once the peer version is parsed (phase → `Kex`).
  fn scan_peer_version(&mut self) -> Result<bool> {
    loop {
      let Some(pos) = self.inbox.iter().position(|&b| b == b'\n') else {
        if self.inbox.len() > MAX_BANNER_LINE {
          return Err(Error::Protocol("banner line too long"));
        }
        return Ok(false);
      };
      let line: Vec<u8> = self.inbox.drain(..=pos).collect();
      self.banner_total = self.banner_total.saturating_add(line.len());
      if self.banner_total > MAX_BANNER_TOTAL_BYTES {
        return Err(Error::Protocol("banner too large"));
      }
      if line.starts_with(b"SSH-") {
        let parsed = VersionExchange::parse_remote(&line)?;
        self.v_s = parsed.into_bytes();
        self.phase = Phase::Kex;
        return Ok(true);
      }
      self.banner_lines += 1;
      if self.banner_lines > MAX_BANNER_LINES {
        return Err(Error::Protocol("peer banner too long"));
      }
    }
  }

  /// Route one decoded transport packet.
  fn route_packet<R: CryptoRngCore>(&mut self, payload: &[u8], rng: &mut R) -> Result<()> {
    let msg = payload.first().copied().unwrap_or(0);
    debug!("ssh: route msg={msg} phase={:?} kexing={}", self.phase, self.runner.is_kexing());
    match payload.first().copied() {
      Some(1) => Err(Error::Protocol("peer sent SSH_MSG_DISCONNECT")),
      Some(2) | Some(3) | Some(4) => Ok(()),
      Some(SSH_MSG_EXT_INFO) => {
        if !self.runner.may_accept_ext_info() {
          return Err(Error::Protocol("unexpected SSH_MSG_EXT_INFO"));
        }
        self.runner.handle_inbound_ext_info(payload)
      }
      Some(b) if is_kex_msg(b) => {
        if b == puressh::transport::kexinit::SSH_MSG_KEXINIT && !self.runner.is_kexing() {
          // The peer kicked off a re-key: restart our runner to match.
          info!("ssh: peer-initiated rekey");
          self.initiate_rekey(rng)?;
        }
        debug!("ssh: routing kex msg {b}");
        self.route_kex(payload, rng)?;
        if self.runner.is_completed() {
          info!("ssh: kex completed");
          if self.phase == Phase::Kex {
            self.session_id = self.runner.session_id().ok_or(Error::Protocol("kex: missing session id"))?.to_vec();
            self.phase = Phase::PostKex;
            self.events.push_back(SshEvent::Connected);
            info!("ssh: starting auth");
            self.start_auth(rng)?;
          }
          self.drain_deferred(rng)?;
        }
        Ok(())
      }
      _ => {
        if self.runner.is_kexing() {
          self.deferred.push_back(payload.to_vec());
          return Ok(());
        }
        self.runner.note_inbound_other();
        self.route_app(payload, rng)
      }
    }
  }

  /// Feed one KEX-stream packet into the runner, building the host-key
  /// verifier on `SSH_MSG_KEX_ECDH_REPLY`, and enqueue its output.
  fn route_kex<R: CryptoRngCore>(&mut self, payload: &[u8], rng: &mut R) -> Result<()> {
    let msg = *payload.first().ok_or(Error::Format("empty kex payload"))?;
    let verifier: Option<Box<dyn HostKeyVerify>> = if msg == SSH_MSG_KEX_ECDH_REPLY {
      if payload.len() < 5 {
        return Err(Error::Format("kex-ecdh-reply too short"));
      }
      let k_s_len = u32::from_be_bytes([payload[1], payload[2], payload[3], payload[4]]) as usize;
      if payload.len() < 5 + k_s_len {
        return Err(Error::Format("kex-ecdh-reply truncated"));
      }
      let k_s = &payload[5..5 + k_s_len];
      let neg = self.runner.negotiated().ok_or(Error::Protocol("kex: no negotiated algorithms"))?;
      // Trust-on-first-use: accept any `ssh-ed25519` host key, but still verify
      // the exchange-hash signature against it.
      Some(host_key_verify_by_name(&neg.host_key, k_s)?)
    } else {
      None
    };
    let v_c = LOCAL_VERSION.as_bytes().to_vec();
    let v_s = self.v_s.clone();
    debug!("ssh: kex runner on_packet msg={msg} (crypto...)");
    let adv = self
      .runner
      .on_packet(rng, &mut self.codec, payload, None, verifier.as_deref(), &v_c, &v_s)?;
    debug!("ssh: kex runner returned {} outbound frames", adv.outbound.len());
    for p in adv.outbound {
      self.enqueue_payload(&p, rng)?;
    }
    Ok(())
  }

  /// Kick off userauth with the single publickey credential.
  fn start_auth<R: CryptoRngCore>(&mut self, rng: &mut R) -> Result<()> {
    let mut auth = ClientAuth::new(self.user.clone(), self.session_id.clone());
    if let Some(hk) = self.host_key.take() {
      auth.add_credential(ClientCredential::PublicKey(hk));
    }
    let first = auth.start();
    self.auth = Some(auth);
    self.enqueue_payload(&first, rng)
  }

  /// Emit a fresh KEXINIT to start a re-key (runner must be `Completed`).
  fn initiate_rekey<R: CryptoRngCore>(&mut self, rng: &mut R) -> Result<()> {
    let advert = build_kexinit(rng);
    let adv = self.runner.restart(rng, advert)?;
    for p in adv.outbound {
      self.enqueue_payload(&p, rng)?;
    }
    Ok(())
  }

  /// Replay application packets buffered during a re-key, in arrival order.
  fn drain_deferred<R: CryptoRngCore>(&mut self, rng: &mut R) -> Result<()> {
    while !self.runner.is_kexing() {
      let Some(payload) = self.deferred.pop_front() else {
        break;
      };
      self.runner.note_inbound_other();
      self.route_app(&payload, rng)?;
    }
    Ok(())
  }

  /// Route a post-NEWKEYS application packet to the auth or channel layer.
  fn route_app<R: CryptoRngCore>(&mut self, payload: &[u8], rng: &mut R) -> Result<()> {
    if self.auth.is_some() {
      let mut auth = self.auth.take().expect("auth present");
      let step = auth.on_packet(payload)?;
      match step {
        ClientStep::Send(p) => {
          self.auth = Some(auth);
          self.enqueue_payload(&p, rng)?;
        }
        ClientStep::Success => {
          self.codec.activate_compress();
          self.runner.arm_ext_info_post_auth();
          // Open a session channel; the shell is requested on confirmation.
          let (id, p) = self.conn.open(ChannelOpen::Session)?;
          self.channel = Some(id);
          self.enqueue_payload(&p, rng)?;
        }
        ClientStep::Failed { .. } => {
          self.events.push_back(SshEvent::AuthFailed);
        }
        ClientStep::Banner { .. } | ClientStep::Idle => {
          self.auth = Some(auth);
        }
      }
      return Ok(());
    }

    let ev = self.conn.on_packet(payload)?;
    self.handle_channel_event(ev, rng)
  }

  /// Handle a decoded channel event for the shell channel.
  fn handle_channel_event<R: CryptoRngCore>(&mut self, ev: ChannelEvent, rng: &mut R) -> Result<()> {
    match ev {
      ChannelEvent::OpenConfirmed { channel } if Some(channel) == self.channel => {
        let p = self.conn.send_request(
          channel,
          ChannelRequest::PtyReq {
            term: "vt100".into(),
            cols: 32,
            rows: 8,
            px_w: 0,
            px_h: 0,
            modes: Vec::new(),
          },
          true,
        )?;
        self.pty_sent = true;
        self.enqueue_payload(&p, rng)?;
      }
      ChannelEvent::Success { channel } if Some(channel) == self.channel => {
        if self.pty_sent && !self.shell_sent {
          let p = self.conn.send_request(channel, ChannelRequest::Shell, true)?;
          self.shell_sent = true;
          self.enqueue_payload(&p, rng)?;
        } else if self.shell_sent {
          self.events.push_back(SshEvent::Ready);
        }
      }
      ChannelEvent::Failure { channel } if Some(channel) == self.channel => {
        // The server refused the PTY (or shell) request. Try a bare shell if
        // we haven't yet, otherwise surface the error.
        if !self.shell_sent {
          let p = self.conn.send_request(channel, ChannelRequest::Shell, true)?;
          self.shell_sent = true;
          self.enqueue_payload(&p, rng)?;
        } else {
          self.events.push_back(SshEvent::Error("channel request refused".into()));
        }
      }
      ChannelEvent::Data { channel, data } if Some(channel) == self.channel => {
        let adj = self.conn.replenish_window(channel, data.len() as u32)?;
        if let Some(p) = adj {
          self.enqueue_payload(&p, rng)?;
        }
        self.events.push_back(SshEvent::Data(data));
      }
      ChannelEvent::ExtendedData { channel, data, .. } if Some(channel) == self.channel => {
        let adj = self.conn.replenish_window(channel, data.len() as u32)?;
        if let Some(p) = adj {
          self.enqueue_payload(&p, rng)?;
        }
        self.events.push_back(SshEvent::Data(data));
      }
      ChannelEvent::Request {
        channel,
        request,
        want_reply,
      } if Some(channel) == self.channel => {
        if let ChannelRequest::ExitStatus { code } = request {
          self.events.push_back(SshEvent::ExitStatus(code));
        }
        if want_reply {
          let p = self.conn.send_request_failure(channel)?;
          self.enqueue_payload(&p, rng)?;
        }
      }
      ChannelEvent::Eof { channel } if Some(channel) == self.channel => {
        if !self.eof_sent {
          if let Ok(p) = self.conn.send_eof(channel) {
            self.enqueue_payload(&p, rng)?;
          }
          self.eof_sent = true;
        }
      }
      ChannelEvent::Close { channel } if Some(channel) == self.channel => {
        self.remote_close = true;
        if !self.close_sent {
          if let Ok(p) = self.conn.send_close(channel) {
            self.enqueue_payload(&p, rng)?;
          }
          self.close_sent = true;
        }
        self.events.push_back(SshEvent::Closed);
      }
      ChannelEvent::OpenFailed {
        channel,
        reason: _,
        description,
      } if Some(channel) == self.channel => {
        self
          .events
          .push_back(SshEvent::Error(format!("session open failed: {description}")));
      }
      _ => {}
    }
    Ok(())
  }
}

/// Build a fresh KEXINIT advert with a random cookie from `rng`.
fn build_kexinit<R: CryptoRngCore>(rng: &mut R) -> KexInit {
  let mut cookie = [0u8; 16];
  rng.fill_bytes(&mut cookie);
  KexInit::from_algorithms(&ALGORITHMS, cookie)
}
