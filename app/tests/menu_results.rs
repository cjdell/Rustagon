//! Tests for Block 9's app-navigation-with-results plumbing:
//!
//! - driver-level: the Files picker and the confirm dialog return the
//!   right `AppAction`s (`Result(Path(..))`, `Result(Confirm(..))`, `Stop`);
//! - end-to-end: `menu_task` pushes the picker from the SSH connect form,
//!   delivers the chosen path back as `AppRunEvent::Result`, and fills the
//!   form field — and delivers `Cancelled` when the picker is dismissed.
//!
//! Run: `just test` (i.e. `cargo test -p app --features testing`).

extern crate std;

use std::string::{String, ToString};

// The only critical-section impl for this test binary. Unlike the golden
// tests (single-threaded, serialized by a global lock), the end-to-end tests
// here run `menu_task` on a background thread that shares `MockPlatform`'s
// embassy-sync queues *and* the embassy-time mock clock with the test thread.
// The lock must therefore be a real mutex — a no-op impl would let the two
// threads race the RefCells inside the mock driver and the channels.
static CS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// `RawRestoreState` is `()` in this build, so the guard is parked in a
// thread-local between `acquire` and `release` (a thread holds at most one
// critical section at a time — `critical_section::with` is not reentrant).
thread_local! {
  static CS_GUARD: std::cell::Cell<Option<std::sync::MutexGuard<'static, ()>>> = const { std::cell::Cell::new(None) };
}

struct TestLock;
critical_section::set_impl!(TestLock);
unsafe impl critical_section::Impl for TestLock {
  unsafe fn acquire() -> critical_section::RawRestoreState {
    CS_GUARD.with(|g| g.set(Some(CS_LOCK.lock().unwrap_or_else(|e| e.into_inner()))));
  }

  unsafe fn release(_restore: critical_section::RawRestoreState) {
    CS_GUARD.with(|g| g.take());
  }
}

use app::apps::confirm::ConfirmationApp;
use app::apps::files::FilesApp;
use app::apps::{AppAction, AppParams, AppResult, MenuAppContext};
use app::menu::state::create_stack_event_handle;
use app::menu::{menu_task, MenuRunnerContext};
use app::protocol::HostIpcChannel;
use app::testing::{AppDriver, MockPlatform};
use app::types::{HexButton, LcdScreen};
use futures::task::LocalSpawnExt;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_ctx(mock: &MockPlatform) -> MenuAppContext<MockPlatform> {
  let channel: &'static HostIpcChannel = Box::leak(Box::new(HostIpcChannel::new()));
  MenuAppContext::new(mock.clone(), channel.sender())
}

/// Poll the mock's last recorded screen until `expect` matches.
fn wait_for(mock: &MockPlatform, what: &str, expect: impl Fn(&LcdScreen) -> bool) {
  let deadline = Instant::now() + Duration::from_secs(15);
  loop {
    if let Some(screen) = mock.last_screen() {
      if expect(&screen) {
        return;
      }
    }
    assert!(
      Instant::now() < deadline,
      "timed out waiting for {what}; last screen: {:?}",
      mock.last_screen()
    );
    std::thread::sleep(Duration::from_millis(5));
  }
}

fn is_text_buffer_with(screen: &LcdScreen, needle: &str) -> bool {
  matches!(screen, LcdScreen::TextBuffer { lines } if lines.iter().any(|l| l.text.contains(needle)))
}

// ---------------------------------------------------------------------------
// Driver-level: the sub-apps return the right actions
// ---------------------------------------------------------------------------

#[test]
fn files_picker_returns_path() {
  let mock = MockPlatform::new();
  mock.seed_file("alpha.key", b"a");
  mock.seed_file("beta.key", b"b");
  let app = FilesApp::<MockPlatform>::with_params(
    make_ctx(&mock),
    AppParams::PickFile {
      message: "Choose a key".into(),
    },
  );
  let mut driver = AppDriver::new(app, &mock);
  driver.settle(0);

  // Fire on the first file returns its path to the parent.
  mock.push_button(HexButton::Fire);
  let action = driver.settle(0);
  assert_eq!(action, Some(AppAction::Result(AppResult::Path("alpha.key".to_string()))));
}

#[test]
fn files_picker_back_is_stop() {
  let mock = MockPlatform::new();
  mock.seed_file("alpha.key", b"a");
  let app = FilesApp::<MockPlatform>::with_params(
    make_ctx(&mock),
    AppParams::PickFile {
      message: "Choose a key".into(),
    },
  );
  let mut driver = AppDriver::new(app, &mock);
  driver.settle(0);

  // Left (back) stops the picker (→ Cancelled for the parent).
  mock.push_button(HexButton::Left);
  let action = driver.settle(0);
  assert_eq!(action, Some(AppAction::Stop));
}

#[test]
fn files_normal_mode_still_opens_detail() {
  // The picker is opt-in: the ordinary Files app is unchanged.
  let mock = MockPlatform::new();
  mock.seed_file("alpha.key", b"a");
  let app = FilesApp::<MockPlatform>::new(make_ctx(&mock));
  let mut driver = AppDriver::new(app, &mock);
  driver.settle(0);

  mock.push_button(HexButton::Fire);
  assert!(driver.settle(0).is_none(), "normal mode stays in the loop");
  // The detail screen is up (Name: alpha.key).
  assert!(matches!(
    mock.last_screen(),
    Some(LcdScreen::Menu { menu, .. }) if menu.iter().any(|l| l.1.contains("Name: alpha.key"))
  ));
}

#[test]
fn confirm_dialog_fire_yes() {
  let mock = MockPlatform::new();
  let app = ConfirmationApp::<MockPlatform>::with_params(
    make_ctx(&mock),
    AppParams::Confirm {
      title: "Host key changed".into(),
      message: "user@host:22".into(),
    },
  );
  let mut driver = AppDriver::new(app, &mock);
  driver.settle(0);

  mock.push_button(HexButton::Fire);
  let action = driver.settle(0);
  assert_eq!(action, Some(AppAction::Result(AppResult::Confirm(true))));
}

#[test]
fn confirm_dialog_no_and_boot() {
  let mock = MockPlatform::new();
  let app = ConfirmationApp::<MockPlatform>::with_params(
    make_ctx(&mock),
    AppParams::Confirm {
      title: "T".into(),
      message: "M".into(),
    },
  );
  let mut driver = AppDriver::new(app, &mock);
  driver.settle(0);

  // Down to [No], fire.
  mock.push_button(HexButton::Down);
  driver.settle(0);
  mock.push_button(HexButton::Fire);
  let action = driver.settle(0);
  assert_eq!(action, Some(AppAction::Result(AppResult::Confirm(false))));

  // Boot stops the dialog (→ Cancelled for the parent).
  let app = ConfirmationApp::<MockPlatform>::new(make_ctx(&mock));
  let mut driver = AppDriver::new(app, &mock);
  driver.settle(0);
  mock.push_boot();
  let action = driver.settle(0);
  assert_eq!(action, Some(AppAction::Stop));
}

// ---------------------------------------------------------------------------
// End-to-end: menu pushes the picker from SSH and delivers the result
// ---------------------------------------------------------------------------

/// SSH connect form → Fire on the key field → Files picker → Fire on a file
/// → the path lands back in the SSH key field; then picker → Left → the
/// form is unchanged (Cancelled).
#[test]
fn menu_delivers_picker_result_to_ssh() {
  let mock = MockPlatform::new();
  mock.seed_file("id_ed255.key", b"-----BEGIN OPENSSH PRIVATE KEY-----fake-----END-----\n");

  let channel: &'static HostIpcChannel = Box::leak(Box::new(HostIpcChannel::new()));
  let runner = MenuRunnerContext {
    platform: mock.clone(),
    host_ipc_sender: channel.sender(),
    stack_event_handle: create_stack_event_handle(),
    app_loader: None,
    additional_apps: &[],
    auto_launch: None,
  };

  // The menu future is `!Send` (TCP pump futures, by design), so it must be
  // created *inside* the worker thread — the same pattern desktop uses.
  let _menu_thread = std::thread::spawn(move || {
    let mut pool = futures::executor::LocalPool::new();
    pool.spawner().spawn_local(menu_task(runner)).unwrap();
    pool.run();
  });

  // 1. The root menu is up; navigate to SSH and fire.
  wait_for(
    &mock,
    "root menu",
    |s| matches!(s, LcdScreen::Menu { menu, .. } if menu.iter().any(|l| l.1 == "SSH")),
  );
  let ssh_index = {
    let LcdScreen::Menu { menu, .. } = mock.last_screen().expect("root menu") else {
      panic!()
    };
    menu.iter().position(|l| l.1 == "SSH").expect("SSH is in the root menu")
  };
  for _ in 0..ssh_index {
    mock.push_button(HexButton::Down);
  }
  mock.push_button(HexButton::Fire);

  // 2. The SSH connect form.
  wait_for(&mock, "ssh connect form", |s| is_text_buffer_with(s, "SSH Client"));

  // 3. Down to the key field, Fire → the Files picker is pushed.
  mock.push_button(HexButton::Down);
  mock.push_button(HexButton::Down);
  mock.push_button(HexButton::Fire);
  wait_for(
    &mock,
    "picker",
    |s| matches!(s, LcdScreen::Menu { menu, .. } if menu.iter().any(|l| l.1.starts_with("id_ed255.key"))),
  );

  // 4. Fire on the file → Path result → the SSH key field shows it.
  mock.push_button(HexButton::Fire);
  wait_for(&mock, "picked key in the form", |s| is_text_buffer_with(s, "key: id_ed255.key"));

  // 5. Picker again (focus stayed on the key field); Left → Cancelled;
  // the form still shows the picked key.
  mock.push_button(HexButton::Fire);
  wait_for(
    &mock,
    "picker (second time)",
    |s| matches!(s, LcdScreen::Menu { menu, .. } if menu.iter().any(|l| l.1.starts_with("id_ed255.key"))),
  );
  mock.push_button(HexButton::Left);
  wait_for(&mock, "back on the form", |s| is_text_buffer_with(s, "SSH Client"));
  assert!(
    is_text_buffer_with(&mock.last_screen().expect("form"), "key: id_ed255.key"),
    "cancelled picker must not change the key field"
  );
}

/// The confirmation dialog reachable from the root menu answers through the
/// menu (Fire on Yes → back at the root; the result is dropped at root).
#[test]
fn menu_confirm_dialog_from_root() {
  let mock = MockPlatform::new();

  let channel: &'static HostIpcChannel = Box::leak(Box::new(HostIpcChannel::new()));
  let runner = MenuRunnerContext {
    platform: mock.clone(),
    host_ipc_sender: channel.sender(),
    stack_event_handle: create_stack_event_handle(),
    app_loader: None,
    additional_apps: &[],
    auto_launch: None,
  };

  // The menu future is `!Send` (TCP pump futures, by design), so it must be
  // created *inside* the worker thread — the same pattern desktop uses.
  let _menu_thread = std::thread::spawn(move || {
    let mut pool = futures::executor::LocalPool::new();
    pool.spawner().spawn_local(menu_task(runner)).unwrap();
    pool.run();
  });

  wait_for(
    &mock,
    "root menu",
    |s| matches!(s, LcdScreen::Menu { menu, .. } if menu.iter().any(|l| l.1 == "Confirm")),
  );
  let confirm_index = {
    let LcdScreen::Menu { menu, .. } = mock.last_screen().expect("root menu") else {
      panic!()
    };
    menu.iter().position(|l| l.1 == "Confirm").expect("Confirm is in the root menu")
  };
  for _ in 0..confirm_index {
    mock.push_button(HexButton::Down);
  }
  mock.push_button(HexButton::Fire);

  // The dialog shows the default prompt.
  wait_for(
    &mock,
    "confirm dialog",
    |s| matches!(s, LcdScreen::Menu { menu, .. } if menu.iter().any(|l| l.1 == "Proceed?")),
  );

  // Fire on Yes → the app pops with a result; root has no pending slot, so
  // the result is dropped and we are back at the root menu.
  mock.push_button(HexButton::Fire);
  wait_for(
    &mock,
    "back at root",
    |s| matches!(s, LcdScreen::Menu { menu, .. } if menu.iter().any(|l| l.1 == "SSH")),
  );
}

/// SSH remembers the connect form in the KV store across relaunch.
#[test]
fn ssh_settings_survive_relaunch() {
  use app::apps::ssh::SshApp;
  use app::types::{DeviceEvent, KeyCode, KeyEventType, KeyboardEvent};

  let mock = MockPlatform::new();

  let ctx = make_ctx(&mock);
  let app = SshApp::<MockPlatform>::new(ctx);
  let mut driver = AppDriver::new(app, &mock);
  driver.settle(0);

  // Append "x" to the host field (focus starts there) and attempt a
  // connect: the settings are saved before the TCP stage (which fails on
  // the mock, taking the app back to the form).
  mock.push_device_event(DeviceEvent::Keyboard(KeyboardEvent {
    port: 1,
    typ: KeyEventType::Pressed,
    code: KeyCode::X,
  }));
  driver.settle(0);
  for _ in 0..4 {
    mock.push_button(HexButton::Down);
    driver.settle(0);
  }
  mock.push_button(HexButton::Fire);
  driver.settle(500);

  // The settings were persisted under /apps/ssh/kv.json.
  let stored = String::from_utf8(mock.storage_read("/apps/ssh/kv.json").expect("kv file written")).unwrap();
  assert!(stored.contains("192.168.49.1x"), "edited host saved: {stored}");
  assert!(stored.contains("id_ed255.key"), "key saved: {stored}");

  // A fresh app instance restores the saved form.
  let app = SshApp::<MockPlatform>::new(make_ctx(&mock));
  let mut driver = AppDriver::new(app, &mock);
  driver.settle(0);
  wait_for(&mock, "restored form", |s| is_text_buffer_with(s, "host: 192.168.49.1x"));
}
