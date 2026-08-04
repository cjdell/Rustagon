//! End-to-end test for the no_std `SshSession` engine.
//!
//! Spins up a real puressh server (host-side, `std`) on a local TCP socket and
//! drives the engine's state machine over a raw stream — exercising version
//! exchange, `curve25519-sha256` KEX, `ssh-ed25519` host-key verification and
//! publickey auth, the session channel, an interactive shell, and the flow of
//! channel data in both directions.

extern crate std;

use super::{SshEvent, SshSession};
use alloc::{
  boxed::Box,
  string::{String, ToString},
  vec,
  vec::Vec,
};
use core::time::Duration;
use purecrypto::rng::{OsRng, RngCore};
use puressh::{
  auth::{AuthAttempt, AuthDecision, Authenticator},
  hostkey::{Ed25519HostKey, HostKey},
  server::{
    AuthenticatorFactory, CommandHandler, Config, ExecResult, PtySpec, Server, SessionEnv, ShellExitStatus, ShellHandler, ShellSession,
  },
};
use std::eprintln;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;

/// Accepts the single client public key.
struct OneKeyAuth {
  user: String,
  blob: Vec<u8>,
}

impl Authenticator for OneKeyAuth {
  fn evaluate(&mut self, attempt: AuthAttempt) -> AuthDecision {
    match attempt {
      AuthAttempt::PublicKey {
        user,
        public_blob,
        probe_only,
        verified,
        ..
      } => {
        if user != self.user || public_blob != self.blob {
          return AuthDecision::Reject;
        }
        if probe_only {
          return AuthDecision::Accept;
        }
        if verified { AuthDecision::Accept } else { AuthDecision::Reject }
      }
      _ => AuthDecision::Reject,
    }
  }
}

/// Not exercised — a shell handler is used instead.
struct DummyCommandHandler;

impl CommandHandler for DummyCommandHandler {
  fn handle(&self, _user: &str, _env: &SessionEnv, _command: &str) -> ExecResult {
    ExecResult {
      stdout: Vec::new(),
      stderr: Vec::new(),
      exit_status: 0,
    }
  }
}

/// A fake shell that echoes everything it receives and exits on `exit`.
struct EchoShell {
  out: Arc<Mutex<Vec<u8>>>,
  seen: Vec<u8>,
  exited: Option<ShellExitStatus>,
}

impl ShellSession for EchoShell {
  fn read(&mut self, buf: &mut [u8]) -> puressh::Result<usize> {
    let mut out = self.out.lock().unwrap();
    let n = out.len().min(buf.len());
    buf[..n].copy_from_slice(&out[..n]);
    out.drain(..n);
    Ok(n)
  }
  fn write(&mut self, buf: &[u8]) -> puressh::Result<usize> {
    self.seen.extend_from_slice(buf);
    if self.seen.windows(4).any(|w| w == b"exit") {
      self.exited = Some(ShellExitStatus::Exited(0));
    }
    self.out.lock().unwrap().extend_from_slice(buf);
    Ok(buf.len())
  }
  fn close_stdin(&mut self) -> puressh::Result<()> {
    Ok(())
  }
  fn resize(&mut self, _cols: u32, _rows: u32, _px_w: u32, _px_h: u32) -> puressh::Result<()> {
    Ok(())
  }
  fn try_exit(&mut self) -> Option<ShellExitStatus> {
    self.exited.clone()
  }
}

struct EchoShellHandler {
  out: Arc<Mutex<Vec<u8>>>,
}

impl ShellHandler for EchoShellHandler {
  fn spawn(&self, _user: &str, _env: &SessionEnv, _pty: Option<PtySpec>) -> puressh::Result<Box<dyn ShellSession>> {
    Ok(Box::new(EchoShell {
      out: self.out.clone(),
      seen: Vec::new(),
      exited: None,
    }))
  }
}

fn fresh_seed() -> [u8; 32] {
  let mut s = [0u8; 32];
  OsRng.fill_bytes(&mut s);
  s
}

#[test]
fn full_handshake_shell_echo_roundtrip() {
  let host_seed = fresh_seed();
  let client_seed = fresh_seed();
  let client_key = Ed25519HostKey::from_seed(client_seed);
  let client_blob = client_key.public_blob();
  let user = "ssh-engine-user".to_string();

  let host_key: Box<dyn HostKey + Send + Sync> = Box::new(Ed25519HostKey::from_seed(host_seed));
  let u = user.clone();
  let b = client_blob.clone();
  let factory: Arc<dyn AuthenticatorFactory> = Arc::new(move || {
    Box::new(OneKeyAuth {
      user: u.clone(),
      blob: b.clone(),
    }) as Box<dyn Authenticator>
  });

  let echo_out = Arc::new(Mutex::new(Vec::new()));
  let mut cfg = Config::new(vec![host_key], factory, vec!["publickey"], Arc::new(DummyCommandHandler));
  cfg.shell_handler = Some(Arc::new(EchoShellHandler { out: echo_out.clone() }));

  let mut server = Server::bind("127.0.0.1:0", cfg).expect("bind");
  let addr = server.local_addr().expect("addr");
  let server_thread = thread::spawn(move || {
    let _ = server.accept_one();
  });

  let mut sock = TcpStream::connect(addr).expect("connect");
  sock.set_read_timeout(Some(Duration::from_millis(20))).unwrap();

  let mut rng = OsRng;
  let mut session = SshSession::new(user.clone(), Box::new(client_key), &mut rng);
  session.start(&mut rng).expect("start");

  let mut echoed: Vec<u8> = Vec::new();
  let mut exit_code: Option<u32> = None;
  let mut sent_hi = false;
  let mut sent_exit = false;
  let mut closed = false;

  for _ in 0..50_000 {
    // Flush outbound frames.
    while let Some(frame) = session.poll_transmit() {
      sock.write_all(&frame).expect("write");
    }
    // Drain engine events.
    while let Some(ev) = session.poll_event() {
      match ev {
        SshEvent::Ready => {
          if !sent_hi {
            session.send_data(b"hi\n", &mut rng).expect("send_data");
            sent_hi = true;
          }
        }
        SshEvent::Data(data) => {
          echoed.extend_from_slice(&data);
          if !sent_exit && echoed.windows(3).any(|w| w == b"hi\n") {
            session.send_data(b"exit\n", &mut rng).expect("send_data");
            sent_exit = true;
          }
        }
        SshEvent::ExitStatus(code) => exit_code = Some(code),
        SshEvent::Closed => closed = true,
        _ => {}
      }
    }

    while let Some(frame) = session.poll_transmit() {
      sock.write_all(&frame).expect("write");
    }

    if closed && sent_hi && exit_code == Some(0) {
      break;
    }

    // Read inbound bytes.
    let mut buf = [0u8; 16 * 1024];
    match sock.read(&mut buf) {
      Ok(0) => break,
      Ok(n) => session.handle_input(&buf[..n], &mut rng).expect("handle_input"),
      Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => continue,
      Err(e) => panic!("read error: {e}"),
    }
  }

  assert!(
    echoed.windows(3).any(|w| w == b"hi\n"),
    "expected the shell to echo 'hi\\n', got {:?}",
    String::from_utf8_lossy(&echoed)
  );
  assert_eq!(exit_code, Some(0), "expected exit status 0, got {exit_code:?}");
  assert!(closed, "expected the session to close");

  drop(sock);
  let _ = server_thread.join();
}

/// Connects the engine to a REAL OpenSSH server (`sshd` on 127.0.0.1:22222
/// with a throwaway ed25519 host key and an authorized client key in
/// `/tmp/sshtest`) — the same scenario the firmware hangs on. Requires the
/// server to already be running; skipped if it isn't.
#[test]
fn handshake_against_real_openssh() {
  use puressh::key::PrivateKey;

  let key_path = std::path::Path::new("/tmp/sshtest/client_key");
  if !key_path.exists() {
    eprintln!("skipping: no /tmp/sshtest/client_key");
    return;
  }
  let pem = std::fs::read_to_string(key_path).expect("read client key");
  let key = PrivateKey::parse_openssh_pem(&pem, None).expect("parse client key");
  let host_key = key.into_host_key().expect("into host key");

  let mut sock = TcpStream::connect("127.0.0.1:22222").expect("connect to sshd");
  sock.set_read_timeout(Some(Duration::from_millis(20))).unwrap();

  let mut rng = OsRng;
  let mut session = SshSession::new("testuser".to_string(), host_key, &mut rng);
  session.start(&mut rng).expect("start");

  let mut ready = false;
  let mut reached_auth = false;
  let mut protocol_error = false;
  let mut inbound_types: Vec<u8> = Vec::new();
  let mut sent_types: Vec<u8> = Vec::new();
  let start = std::time::Instant::now();
  for _ in 0..100_000 {
    if start.elapsed() > std::time::Duration::from_secs(10) {
      eprintln!("openssh: TIMED OUT after 10s; inbound msg types {inbound_types:?}, outbound {sent_types:?}");
      break;
    }
    while let Some(frame) = session.poll_transmit() {
      if !frame.is_empty() {
        sent_types.push(frame[0]);
      }
      sock.write_all(&frame).expect("write");
    }
    while let Some(ev) = session.poll_event() {
      match ev {
        SshEvent::Ready => {
          eprintln!("openssh: READY (handshake + auth + shell open)");
          ready = true;
        }
        // Auth failing (e.g. invalid user on the throwaway host) still proves
        // the KEX/host-key/transport completed — the point of this test.
        SshEvent::AuthFailed => {
          eprintln!("openssh: AUTH FAILED (KEX completed; expected on throwaway host)");
          reached_auth = true;
        }
        SshEvent::Error(msg) => {
          eprintln!("openssh: error: {msg}");
          protocol_error = true;
        }
        SshEvent::Closed => {
          eprintln!("openssh: closed");
          protocol_error = true;
        }
        _ => {}
      }
    }
    if ready || reached_auth || protocol_error {
      break;
    }
    let mut buf = [0u8; 16 * 1024];
    match sock.read(&mut buf) {
      Ok(0) => {
        eprintln!("openssh: EOF from server");
        break;
      }
      Ok(n) => {
        if n > 0 {
          inbound_types.push(buf[4]);
        }
        if let Err(e) = session.handle_input(&buf[..n], &mut rng) {
          eprintln!("openssh: handle_input error: {e:?}");
          protocol_error = true;
          break;
        }
      }
      Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => continue,
      Err(e) => panic!("read error: {e}"),
    }
  }
  eprintln!(
    "openssh: final state ready={ready} reached_auth={reached_auth} protocol_error={protocol_error} inbound={inbound_types:?} outbound={sent_types:?}"
  );
  assert!(
    ready || reached_auth,
    "expected the SSH handshake to reach auth (KEX completed), got ready={ready} reached_auth={reached_auth} protocol_error={protocol_error}"
  );
}
