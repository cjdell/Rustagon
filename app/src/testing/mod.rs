//! Headless test support: a scripted [`MockPlatform`] and an [`AppDriver`]
//! that runs a [`MenuApp`]'s `run()` loop against a script, plus golden-screen
//! helpers.
//!
//! Enabled only by the `testing` feature (`cargo test -p app --features testing`),
//! which also turns on the `embassy-time` mock driver so tests can advance the
//! clock deterministically. Never built into the production firmware or desktop
//! binaries.
//!
//! - [`mock`] — `MockPlatform` and its fake managers (scripted input, in-memory
//!   storage/config, canned HTTP/TCP, a recording display).
//! - [`driver`] — `AppDriver`, a poll-based driver for a single app's `run()`.

pub mod driver;
pub mod mock;

pub use driver::{AppDriver, PollResult};
pub use mock::{HttpScript, MockPlatform, MockStorage, MockTcp};
