//! Golden-screen tests for every built-in app, driven headlessly against
//! [`MockPlatform`] (see `app/src/testing`).
//!
//! Each test runs an app's `run()` loop through [`AppDriver`], injects scripted
//! input / advances the embassy-time mock clock, and asserts the resulting
//! [`LcdScreen`] against a golden JSON fixture under `tests/golden/`.
//!
//! Run: `just test` (i.e. `cargo test -p app --features testing`).
//! Regenerate fixtures: `UPDATE_GOLDEN=1 cargo test -p app --features testing`.

extern crate std;

use std::sync::Mutex;

// The only critical-section impl for this test binary. The embassy-time mock
// driver and the `embassy_sync` primitives inside `MockPlatform` both require
// one (a no-op lock is safe: every test is single-threaded and serialized by
// `GOLDEN_LOCK`).
struct TestLock;
critical_section::set_impl!(TestLock);
// `RawRestoreState` is `()`, so `acquire`'s body is a unit expression — the
// same pattern as the lib's own `TestLock` (see `utils/sync.rs`).
#[allow(clippy::unused_unit)]
unsafe impl critical_section::Impl for TestLock {
  unsafe fn acquire() -> critical_section::RawRestoreState {
    ()
  }
  unsafe fn release(_restore_state: critical_section::RawRestoreState) {}
}

// The embassy-time `MockDriver` is a process-global. Serialize every test that
// advances it so parallel test threads cannot corrupt each other's clocks.
static GOLDEN_LOCK: Mutex<()> = Mutex::new(());

use app::apps::app_store::AppStoreApp;
use app::apps::config::ConfigApp;
use app::apps::editor::EditorApp;
use app::apps::files::FilesApp;
use app::apps::hexpansion_viewer::HexpansionViewerApp;
use app::apps::input_test::InputTestApp;
use app::apps::ota_updater::OtaUpdaterApp;
use app::apps::power_info::PowerInfoApp;
use app::apps::ssh::SshApp;
use app::apps::wifi_scanner::WifiScannerApp;
use app::apps::{AppAction, MenuApp, MenuAppContext};
use app::menu::menus::get_root_menu_options;
use app::menu::RootMenuApp;
use app::platform::Platform;
use app::protocol::HostIpcChannel;
use app::testing::{AppDriver, MockPlatform};
use app::types::{DeviceEvent, HexButton, HexpansionInfo, KeyCode, KeyEventType, KeyboardEvent, LcdScreen, WifiResult};
use app::platform::WifiStatus;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn golden_dir() -> std::path::PathBuf {
  std::env::current_dir().unwrap().join("tests").join("golden")
}

/// Compare `screen` against `tests/golden/<name>`. Under `UPDATE_GOLDEN=1`,
/// (re)write the fixture instead of asserting.
fn expect_screen(name: &str, screen: &LcdScreen) {
  let path = golden_dir().join(name);
  let current = serde_json::to_string_pretty(screen).expect("serialize screen");
  if std::env::var("UPDATE_GOLDEN").is_ok() {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).expect("create golden dir");
    }
    std::fs::write(&path, format!("{current}\n")).expect("write golden");
    eprintln!("wrote golden {name}");
  } else {
    let golden = std::fs::read_to_string(&path).unwrap_or_else(|e| {
      panic!(
        "missing golden fixture {path:?} ({e}); run `UPDATE_GOLDEN=1 cargo test -p app --features testing`"
      )
    });
    assert_eq!(golden.trim_end(), current.trim_end(), "golden mismatch for {name}");
  }
}

fn make_ctx(mock: &MockPlatform) -> MenuAppContext<MockPlatform> {
  // Leaked for a `'static` lifetime (mirrors the desktop/firmware setup); the
  // sender handed to the app is a `Sender<'static>`.
  let channel: &'static HostIpcChannel = Box::leak(Box::new(HostIpcChannel::new()));
  let sender = channel.sender();
  MenuAppContext::new(mock.clone(), sender)
}

/// Construct a driver for `app`, first signalling the app's initial
/// `render()` (what the menu shows on entry) so apps that don't render at the
/// top of `run()` still have a recorded launch screen.
fn make_driver<'a>(mock: &'a MockPlatform, app: impl MenuApp<MockPlatform> + 'static) -> AppDriver<'a, MockPlatform> {
  let screen = app.render();
  let _ = mock.display_manager().signal(screen);
  AppDriver::new(app, mock)
}

/// Run `f` under the golden lock with a freshly-reset mock clock.
fn with_clock<F: FnOnce()>(f: F) {
  // Recover the lock if a prior test poisoned it (keeps failures independent).
  let _guard = GOLDEN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
  embassy_time::MockDriver::get().reset();
  f();
}

fn kb(code: KeyCode, typ: KeyEventType) -> DeviceEvent {
  DeviceEvent::Keyboard(KeyboardEvent { port: 1, typ, code })
}

// ---------------------------------------------------------------------------
// Root menu (acceptance: up/down/fire navigation)
// ---------------------------------------------------------------------------

#[test]
fn root_menu_navigation() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let ctx = make_ctx(&mock);
    let options = get_root_menu_options::<MockPlatform>(&[]);
    let app = RootMenuApp::new(ctx, options);
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("root/launch.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::Down);
    driver.settle(0);
    expect_screen("root/down.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::Down);
    driver.settle(0);
    expect_screen("root/down2.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::Up);
    driver.settle(0);
    // Back to the second option (selected == 1).
    expect_screen("root/up.json", mock.last_screen().as_ref().unwrap());
  });
}

#[test]
fn root_menu_fire_launches_app() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let ctx = make_ctx(&mock);
    let options = get_root_menu_options::<MockPlatform>(&[]);
    let app = RootMenuApp::new(ctx, options);
    let mut driver = make_driver(&mock, app);
    driver.settle(0);

    // Fire on the first option ("App Store") returns a LoadMenuApp action.
    mock.push_button(HexButton::Fire);
    let action = driver.settle(0);
    assert_eq!(action, Some(AppAction::LoadMenuApp("App Store".to_string())));
  });
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[test]
fn config_launch_and_navigate() {
  with_clock(|| {
    let mock = MockPlatform::new();
    mock.set_wifi_status(WifiStatus::Offline);
    mock.seed_file("demo.wsm", &[0u8; 2048]);
    let app = ConfigApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("config/launch.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::Down);
    driver.settle(0);
    expect_screen("config/down.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::Up);
    driver.settle(0);
    expect_screen("config/up.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// WiFi Scanner
// ---------------------------------------------------------------------------

#[test]
fn wifi_scanner_launch_and_navigate() {
  with_clock(|| {
    let mock = MockPlatform::new();
    mock.set_wifi_scan(vec![
      WifiResult { ssid: "HomeNet".into(), signal_strength: -45, password_required: true },
      WifiResult { ssid: "Coffee".into(), signal_strength: -65, password_required: false },
      WifiResult { ssid: "Neighbour".into(), signal_strength: -80, password_required: true },
    ]);
    let app = WifiScannerApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("wifi/launch.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::Down);
    driver.settle(0);
    expect_screen("wifi/down.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

#[test]
fn files_launch_and_open_detail() {
  with_clock(|| {
    let mock = MockPlatform::new();
    mock.seed_file("demo.wsm", &[0u8; 1024]);
    mock.seed_file("notes.txt", b"hello".as_slice());
    let app = FilesApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("files/launch.json", mock.last_screen().as_ref().unwrap());

    // Open the first file's detail view.
    mock.push_button(HexButton::Fire);
    driver.settle(0);
    expect_screen("files/detail.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// Power Info
// ---------------------------------------------------------------------------

#[test]
fn power_info_launch() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let app = PowerInfoApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("power/launch.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// Input Test
// ---------------------------------------------------------------------------

#[test]
fn input_test_launch_and_press() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let app = InputTestApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("input/launch.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::Fire);
    driver.settle(0);
    // "Fire" drops out of the remaining list.
    expect_screen("input/fire.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// Hexpansion Viewer
// ---------------------------------------------------------------------------

#[test]
fn hexpansion_viewer_launch() {
  with_clock(|| {
    let mock = MockPlatform::new();
    // A filled slot makes the app skip its retry-sleep loop.
    let mut slots: Vec<(u8, Option<HexpansionInfo>)> = (0..6).map(|p| (p, None)).collect();
    slots[1] = (
      1,
      Some(HexpansionInfo { port: 1, vid: 0xBAD3, pid: 0x4EEB, unique_id: 42, friendly_name: "KeebDeck".into() }),
    );
    mock.set_hexpansion_state(slots);
    let app = HexpansionViewerApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("hexpansion/launch.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

#[test]
fn editor_launch_and_type() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let app = EditorApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("editor/launch.json", mock.last_screen().as_ref().unwrap());

    // Type a few characters on the first line (raw keyboard events).
    for code in [KeyCode::H, KeyCode::E, KeyCode::L, KeyCode::L, KeyCode::O] {
      mock.push_device_event(kb(code, KeyEventType::Pressed));
      driver.settle(0);
    }
    expect_screen("editor/hello.json", mock.last_screen().as_ref().unwrap());
  });
}

/// Exercises the full editing surface the port to `ui::text_input` must keep
/// identical: typing, shift (uppercase), left/right/home/end, backspace,
/// Enter (line split), and line navigation.
#[test]
fn editor_editing() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let app = EditorApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);

    // Type "HIA" — the A is shifted (Shift + letter → uppercase).
    for (code, shifted) in [(KeyCode::H, false), (KeyCode::I, false), (KeyCode::A, true)] {
      if shifted {
        mock.push_device_event(kb(KeyCode::Shift, KeyEventType::Pressed));
      }
      mock.push_device_event(kb(code, KeyEventType::Pressed));
      driver.settle(0);
      if shifted {
        mock.push_device_event(kb(KeyCode::Shift, KeyEventType::Released));
        driver.settle(0);
      }
    }
    expect_screen("editor/typing.json", mock.last_screen().as_ref().unwrap());

    // Left, left, backspace ("HIA" -> "IA"), right, End, type "B" ("IAB").
    mock.push_button(HexButton::Left);
    driver.settle(0);
    mock.push_button(HexButton::Left);
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::Backspace, KeyEventType::Pressed));
    driver.settle(0);
    mock.push_button(HexButton::Right);
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::End, KeyEventType::Pressed));
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::B, KeyEventType::Pressed));
    driver.settle(0);
    expect_screen("editor/midline.json", mock.last_screen().as_ref().unwrap());

    // Enter splits the line at the cursor; typing lands on the new line.
    mock.push_button(HexButton::Fire);
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::C, KeyEventType::Pressed));
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::D, KeyEventType::Pressed));
    driver.settle(0);
    expect_screen("editor/second_line.json", mock.last_screen().as_ref().unwrap());

    // Up to the first line, Home, type "x" at column 0.
    mock.push_button(HexButton::Up);
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::Home, KeyEventType::Pressed));
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::X, KeyEventType::Pressed));
    driver.settle(0);
    expect_screen("editor/home_insert.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// App Store
// ---------------------------------------------------------------------------

#[test]
fn app_store_launch() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let app = AppStoreApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("app_store/launch.json", mock.last_screen().as_ref().unwrap());
  });
}

#[test]
fn app_store_load_manifest() {
  with_clock(|| {
    let mock = MockPlatform::new();
    // The app requests `<app_store_url>/manifest.json`.
    mock.http().json("manifest.json", r#"[{"name":"fetch","size":1234},{"name":"barefill","size":1024}]"#);
    let app = AppStoreApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);

    mock.push_button(HexButton::HexB);
    driver.settle(0);
    expect_screen("app_store/list.json", mock.last_screen().as_ref().unwrap());

    // Move the selection down one.
    mock.push_button(HexButton::Down);
    driver.settle(0);
    expect_screen("app_store/list_down.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// Firmware Update (OTA)
// ---------------------------------------------------------------------------

#[test]
fn ota_launch_and_prompt() {
  with_clock(|| {
    let mock = MockPlatform::new();
    mock.http().json("version.json", r#"{"version":2,"size":4096}"#);
    let app = OtaUpdaterApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("ota/launch.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::HexB);
    driver.settle(0);
    expect_screen("ota/prompt.json", mock.last_screen().as_ref().unwrap());
  });
}

// ---------------------------------------------------------------------------
// SSH
// ---------------------------------------------------------------------------

#[test]
fn ssh_launch_and_navigate() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let app = SshApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);
    expect_screen("ssh/launch.json", mock.last_screen().as_ref().unwrap());

    mock.push_button(HexButton::Down);
    driver.settle(0);
    expect_screen("ssh/down.json", mock.last_screen().as_ref().unwrap());
  });
}

#[test]
fn ssh_key_not_found() {
  with_clock(|| {
    let mock = MockPlatform::new();
    mock.tcp_connect_ok(true);
    // The key file "id_ed255.key" is intentionally NOT seeded.
    let app = SshApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);
    driver.settle(0);

    // Move the active field down to "[Connect]" (4 fields) and fire.
    for _ in 0..4 {
      mock.push_button(HexButton::Down);
      driver.settle(0);
    }
    mock.push_button(HexButton::Fire);
    driver.settle(0);
    // TCP connects, key read fails -> back on the connect form with a status.
    expect_screen("ssh/key_not_found.json", mock.last_screen().as_ref().unwrap());
  });
}

/// Field editing + focus navigation on the connect form (the port to
/// `ui::form`/`ui::text_input` must keep all of it identical): typing,
/// shift, backspace, Down through the fields, and Fire on a field row (a
/// no-op — only the action row fires).
#[test]
fn ssh_form_editing() {
  with_clock(|| {
    let mock = MockPlatform::new();
    let app = SshApp::<MockPlatform>::new(make_ctx(&mock));
    let mut driver = make_driver(&mock, app);

    // host field: append "x", a shifted "A", then backspace the A.
    mock.push_device_event(kb(KeyCode::X, KeyEventType::Pressed));
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::Shift, KeyEventType::Pressed));
    mock.push_device_event(kb(KeyCode::A, KeyEventType::Pressed));
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::Shift, KeyEventType::Released));
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::Backspace, KeyEventType::Pressed));
    driver.settle(0);
    expect_screen("ssh/host_edited.json", mock.last_screen().as_ref().unwrap());

    // Down x3 to the port field ("22"): backspace -> "2", type "3" -> "23".
    for _ in 0..3 {
      mock.push_button(HexButton::Down);
      driver.settle(0);
    }
    mock.push_device_event(kb(KeyCode::Backspace, KeyEventType::Pressed));
    driver.settle(0);
    mock.push_device_event(kb(KeyCode::Digit3, KeyEventType::Pressed));
    driver.settle(0);
    expect_screen("ssh/port_edited.json", mock.last_screen().as_ref().unwrap());

    // Fire on a field row is a no-op (no connect attempt, no status change).
    mock.push_button(HexButton::Fire);
    driver.settle(0);
    expect_screen("ssh/fire_on_field.json", mock.last_screen().as_ref().unwrap());
  });
}
