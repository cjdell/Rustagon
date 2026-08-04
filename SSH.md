# SSH Client (Rustagon) — Status & Lessons Learned

This file records what has been built for the SSH client app, the architecture,
and the hard-won lessons from debugging it on the firmware. Read this before
continuing any SSH work.

## Status (as of the last session)

- **Desktop**: works end-to-end against a real OpenSSH 10.3 server (the user's
  phone hotspot at 192.168.49.1:22) — full handshake + ed25519 publickey auth +
  interactive shell on the 8-line display.
- **Firmware**: builds (`just build_firmware`). The TCP pump was fixed (mutex
  starvation, below) and the SSH engine reaches the point of processing the
  server's `ECDH_REPLY`. A crash in that crypto step was diagnosed as an
  Xtensa codegen bug and a workaround was applied (see below), **but the fix
  has NOT yet been re-flashed and verified on hardware**. The user needs to
  `just run_firmware` and confirm the handshake completes.
- `app` crate: 14 tests pass — 7 terminal renderer, 1 engine end-to-end
  (against puressh's std server), 1 real-OpenSSH interop, 5 keys.

## Goal & constraints

A no_std SSH client app in the `app` crate, using `puressh`, keeping the
firmware small by supporting only 25519-family crypto.

## Architecture

### Crate layout

- `app/src/ssh/mod.rs` — `SshSession`: the no_std SSH engine (handshake, auth,
  shell channel). `PlatformRng` adapts `Platform::entropy()` to purecrypto's
  RNG traits. 25519-only `ALGORITHMS` const.
- `app/src/ssh/terminal.rs` — minimal VT renderer (CR/LF/BS/TAB, ANSI-CSI
  swallow, cursor overwrite) + key→byte mapping. `DISPLAY_LINES = 8`.
- `app/src/ssh/tests.rs` — engine e2e test + `handshake_against_real_openssh`
  host test (needs a throwaway sshd, see "Testing").
- `app/src/apps/ssh.rs` — `SshApp` menu app: connect screen (host/user/key/
  port fields) → connecting → terminal.
- `app/src/platform/tcp.rs` — `TcpClient` trait, `TcpHandle`, `TcpEvent`,
  `TcpEventChannel` (capacity 16, must be `&'static` — leaked per session).
- `Platform` trait gained `tcp_client() -> Option<TcpHandle>` and
  `entropy(&mut [u8])`.
- `firmware/src/platform/tcp.rs` — `HardwareTcpClient` (embassy-net).
- `desktop/src/platform/tcp.rs` — `DesktopTcpClient` (std `TcpStream` +
  reader thread).

### SSH engine (app, no_std)

`puressh`'s `ClientDriver` / `client` module is gated behind its **`std`-only**
`client` feature, so it is unavailable on the firmware. The engine drives the
sans-IO primitives directly:

- `VersionExchange`, `PacketCodec`, `KexRunner` (`Role::Client`),
  `ClientAuth` (`ClientCredential::PublicKey`), `ConnectionState`.
- It is pure state machine: caller feeds bytes (`handle_input`), drains frames
  (`poll_transmit`), reads events (`poll_event`), and supplies
  `&mut impl CryptoRngCore` + nothing else (no clock). `PlatformRng` wraps
  `platform.entropy()`.
- Reference for the orchestration: puressh's own driver source at
  `~/.cargo/registry/src/*/puressh-0.1.3/src/driver/client.rs` (it is the same
  flow, just std-gated).

Advertised algorithms (`ALGORITHMS`):

```rust
kex:     ["curve25519-sha256", "curve25519-sha256@libssh.org",
          kex-strict-c-v00@openssh.com, ext-info-c]
hostkey: ["ssh-ed25519"]
ciphers: ["chacha20-poly1305@openssh.com", "aes128-ctr"]
macs:    ["hmac-sha2-256-etm@openssh.com", "hmac-sha2-512-etm@openssh.com",
          "hmac-sha2-256"]
comp:    ["none"]
```

Host keys are trust-on-first-use (signature verified, key not persisted). Keys
are loaded from the badge filesystem via `puressh::key::PrivateKey::parse_openssh_pem`.

### TCP transport

```rust
pub trait TcpClient: Send + Sync + fmt::Debug {
  fn connect(&self, host: String, port: u16, channel: &'static TcpEventChannel)
    -> Pin<Box<dyn Future<Output = ()> + 'static>>;   // NOTE: no + Send
  fn send(&self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
  fn close(&self) -> Pin<Box<dyn Future<Output = ()> + 'static>>;
}
```

`TcpEvent` = `Connected | Data(Vec<u8>) | Closed | Error`.

**Firmware**: `HardwareTcpClient` uses an embassy-net `TcpClient` (pool
`TcpClientState::<1, 2048, 2048>`, leaked to `'static`). The **pump task owns
the `TcpConnection` exclusively**; `send`/`close` queue
`TcpCommand::{Send, Close}` on a `Channel<_, _, 8>` that the pump drains
(`select(read, cmd.receive())`). The pump is spawned with
`unsafe { Spawner::for_current_executor() }.await` (non-Send). A `(u64,
Option<&'static CmdChannel>)` slot with a generation counter prevents a stale
pump from clearing a newer connection's channel.

**Desktop**: `connect()` spawns a `std::thread` reader using a
`TcpStream::try_clone()`; the writer is `Arc<Mutex<Option<TcpStream>>>`. No
shared socket mutex between reader and writer, so no starvation.

## Lessons learned (the hard parts)

### 1. `TcpConnection` / `Stack` are `!Send` — futures must not be `+ Send`

`embassy_net::Stack<'static>` and `TcpConnection` hold `UnsafeCell`s (and
`NonNull`), so they are `!Send + !Sync`, and **any future that borrows them is
`!Send`**. The original `TcpClient` trait returned `+ Send` futures and the
firmware failed to compile with "`UnsafeCell<MaybeUninit<…>>` cannot be shared
between threads safely". Fix: the trait futures are `+ 'static` only (no
`+ Send`). The trait object stays `Send + Sync` (needed by `Platform:
Send + Sync`); the firmware menu task is spawned non-Send anyway.

### 2. Mutex starvation on the single-core executor (the "desktop works, firmware hangs" bug)

The first firmware pump held a shared connection `Mutex` across its blocking
`read` and released it on a 100 ms timer. Because the cooperative executor
re-acquires the lock **in the same poll cycle** as the drop, the app's `send()`
was starved forever: the ECDH_INIT (48 bytes) was logged as sent but **never
actually written**, the server waited, and the pump logged idle ticks forever.
The desktop reader is a separate thread with its own socket clone, so it never
contended and always worked.

Lesson: on a single-core cooperative runtime, never hold a shared lock across a
blocking await and then immediately re-acquire it — parked waiters starve.
**Fix**: the pump owns the connection exclusively and `send`/`close` go through
a command channel. No shared socket lock at all.

### 3. Xtensa codegen bug in the constant-time curve25519 (current open issue)

After the starvation fix, the firmware crashed with `LoadProhibited` in
`purecrypto::ec::curve25519::field::Field::point_double`, called from
`puressh::KexRunner::handle_kex_reply_message` (X25519 shared-secret
derivation). The faulting pointer was garbage. The **same packet flow passes on
x86_64 with the exact same `release-lto` profile** (`opt-level='z'` + fat LTO),
so it is an Xtensa-backend codegen bug in the size-optimized constant-time code,
not an engine/protocol bug.

**Applied workaround** (in root `Cargo.toml`):

```toml
[profile.release-lto.package.puressh]
opt-level = 0
[profile.release-lto.package.purecrypto]
opt-level = 0
```

This keeps the crypto frames small and the codegen simple. **Needs on-hardware
verification.** If it still crashes, the next suspect is `-Z stack-protector=all`
in `firmware/.cargo/config.toml` (it instruments every function; Xtensa canary
codegen could be the corruptor) — try removing it.

### 4. Algorithm/MAC negotiation details

- MAC mismatch is NOT a blocker when the negotiated cipher is AEAD
  (`chacha20-poly1305`): puressh skips MAC for AEAD (kexinit.rs `negotiate`),
  and OpenSSH does too. The phone's hardened sshd advertises only `-etm` MACs;
  macOS sshd advertises plain MACs too — both work once AEAD is chosen.
- Strict-kex (`kex-strict-*-v00@openssh.com`) is **optional**: OpenSSH only
  enables it when both sides advertise the matching marker (kex.c: "kex_strict
  = kexalgs_contains(peer, …)"). We advertise `kex-strict-c-v00@openssh.com`
  and `ext-info-c` for correct modern-OpenSSH interop; harmless.
- The phone's sshd had a **custom** KEX list (included
  `diffie-hellman-group-exchange-sha256` instead of the stock group16/18/14),
  which initially looked suspicious — but negotiation still picked
  `curve25519-sha256`, so it was not the problem.

### 5. RNG must come from the platform

The `KexRunner`/codec need a CSPRNG on every call. `PlatformRng` wraps
`Platform::entropy()`. Firmware: `esp_hal::rng::Rng::new().random()` per 4-byte
chunk (the unit-struct TRNG read can't hang). Desktop: `getrandom`. Do not try
to seed a deterministic PRNG on the device.

### 6. Bounded leaks are fine

The `TcpEventChannel`, the `TcpClientState` pool, and the firmware `CmdChannel`
are `Box::leak`ed to `'static` per session (a few KB). The pump exits on
Close/EOF/error and clears the cmd slot; a session aborted via the boot button
(mid-session) leaks the pump until the peer closes — accepted for now.

### 7. Debug logging is in place

`info!` logs throughout `firmware/src/platform/tcp.rs` (resolved/connected/cmd
slot/pump spawn/read/write) and in the engine's `handle_input`/`route_packet`/
`route_kex` (`ssh: decoded msg=…`, `ssh: kex runner on_packet msg=…`,
`ssh: kex completed`, `ssh: starting auth`). These were essential for the
starvation and codegen debugging — keep them until the firmware is verified.

## Testing

- **Firmware build**: `just build_firmware` (sources `~/export-esp.sh`; Xtensa
  toolchain under `~/.rustup/toolchains/esp/`). `just run_firmware` builds +
  flashes + monitors. Firmware cannot be compiled without the Xtensa target.
- **Host engine tests** (in `app/`): `cargo test --features wasm-runtime`.
  `cargo test --profile release-lto --features wasm-runtime openssh` verifies
  the crypto under the firmware's exact LTO/opt-level settings on x86_64.
- **Real-OpenSSH interop test** (`handshake_against_real_openssh`): connects to
  a throwaway sshd. Setup: `ssh-keygen` host + client ed25519 keys, an
  `authorized_keys`, a minimal sshd_config (see the test), then
  `/usr/sbin/sshd -f <config> -D -e`. macOS ships OpenSSH 10.2. The test
  asserts the handshake reaches auth (KEX/host-key verified); auth itself is
  expected to fail for the throwaway user.
- **Desktop**: `just run_desktop_app` / `cargo run` in `desktop/`.

## Caveats / rough edges

- Default key path in the app is `/ssh/id_ed25519`; the user's actual key was
  `id_ed255.key` — the path is user-editable on the connect screen.
- The app shows "Handshake timed out" after 15 s if the server never responds.
- Only publickey auth is implemented (ed25519 private key from the filesystem,
  unencrypted OpenSSH PEM). No password auth.
- The verbose `info!` logging should be trimmed to `debug!` once the firmware
  is confirmed working.
- `AGENTS.md` has not been updated with the new `tcp_client()`/`entropy()`
  platform methods or the `ssh` module — worth doing once firmware is verified.

## Files

- Engine: `app/src/ssh/mod.rs`, `app/src/ssh/terminal.rs`, `app/src/ssh/tests.rs`
- App: `app/src/apps/ssh.rs`, registered in `app/src/apps/mod.rs`
- Platform trait: `app/src/platform/traits.rs`, `app/src/platform/tcp.rs`
- Firmware TCP: `firmware/src/platform/tcp.rs` (+ `hardware.rs`, `mod.rs`,
  `bin/rustagon.rs` wiring)
- Desktop TCP: `desktop/src/platform/tcp.rs` (+ `mod.rs`)
- Crypto profile workaround: root `Cargo.toml`
