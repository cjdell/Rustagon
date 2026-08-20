//! KV store round-trip tests over the in-memory `MockStorage` (see
//! `app/src/kv.rs` for the format + atomicity notes).
//!
//! Run: `just test` (i.e. `cargo test -p app --features testing`).

extern crate alloc;

// The only critical-section impl for this test binary (MockStorage's
// embassy-sync mutex needs one; a no-op lock is safe: the test is
// single-threaded).
struct TestLock;
critical_section::set_impl!(TestLock);
#[allow(clippy::unused_unit)]
unsafe impl critical_section::Impl for TestLock {
  unsafe fn acquire() -> critical_section::RawRestoreState {
    ()
  }
  unsafe fn release(_restore_state: critical_section::RawRestoreState) {}
}

use alloc::string::{String, ToString};
use app::kv::{KvError, KvNamespace};
use app::platform::Platform;
use app::testing::MockPlatform;
use futures::executor::block_on;
use serde::{Deserialize, Serialize};

// A typed value with a nested shape, to prove the serde round trip.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct Point {
  x: u32,
  label: String,
}

fn ns(mock: &MockPlatform, name: &str) -> KvNamespace {
  KvNamespace::new(mock.storage_manager(), name)
}

#[test]
fn set_get_round_trip() {
  let mock = MockPlatform::new();
  let kv = ns(&mock, "ssh");

  block_on(async {
    assert_eq!(kv.get::<Point>("p").await.unwrap(), None);
    kv.set(
      "p",
      Point {
        x: 7,
        label: "seven".into(),
      },
    )
    .await
    .unwrap();
    let got: Point = kv.get("p").await.unwrap().expect("present");
    assert_eq!(
      got,
      Point {
        x: 7,
        label: "seven".into()
      }
    );
  });

  // The namespace file lives under /apps/<name>/.
  let stored = mock.storage_read("/apps/ssh/kv.json").expect("kv file written");
  let text = String::from_utf8(stored).unwrap();
  assert!(text.contains("\"p\""), "file should hold the key: {text}");
}

#[test]
fn set_overwrites_and_remove_deletes() {
  let mock = MockPlatform::new();
  let kv = ns(&mock, "app");

  block_on(async {
    kv.set("k", 1u32).await.unwrap();
    kv.set("k", 2u32).await.unwrap();
    assert_eq!(kv.get::<u32>("k").await.unwrap(), Some(2));

    kv.remove("k").await.unwrap();
    assert_eq!(kv.get::<u32>("k").await.unwrap(), None);
    // Removing an absent key is not an error.
    kv.remove("k").await.unwrap();
  });
}

#[test]
fn namespaces_are_independent() {
  let mock = MockPlatform::new();
  let a = ns(&mock, "a");
  let b = ns(&mock, "b");

  block_on(async {
    a.set("k", "from-a").await.unwrap();
    b.set("k", "from-b").await.unwrap();
    assert_eq!(a.get::<String>("k").await.unwrap(), Some("from-a".to_string()));
    assert_eq!(b.get::<String>("k").await.unwrap(), Some("from-b".to_string()));

    b.remove("k").await.unwrap();
    assert_eq!(a.get::<String>("k").await.unwrap(), Some("from-a".to_string()));
  });
}

#[test]
fn type_mismatch_is_a_decode_error() {
  let mock = MockPlatform::new();
  let kv = ns(&mock, "app");

  block_on(async {
    kv.set("k", "a string").await.unwrap();
    let err = kv.get::<u32>("k").await.unwrap_err();
    assert!(matches!(err, KvError::Decode(_)), "got {err:?}");
    // The value is still intact.
    assert_eq!(kv.get::<String>("k").await.unwrap(), Some("a string".to_string()));
  });
}

#[test]
fn corrupt_file_is_treated_as_empty() {
  let mock = MockPlatform::new();
  let kv = ns(&mock, "app");

  block_on(async {
    kv.set("k", 1u32).await.unwrap();
  });
  // Simulate a torn write: garbage in the namespace file.
  mock.seed_file("/apps/app/kv.json", b"this is not json{{{");

  block_on(async {
    assert!(kv.contains("k").await == false);
    assert_eq!(kv.get::<u32>("k").await.unwrap(), None);
    // And the namespace is usable again (the torn file is replaced).
    kv.set("k", 2u32).await.unwrap();
    assert_eq!(kv.get::<u32>("k").await.unwrap(), Some(2));
  });
}

#[test]
fn multiple_keys_share_one_file() {
  let mock = MockPlatform::new();
  let kv = ns(&mock, "ssh");

  block_on(async {
    // The shape of the real SSH usage: settings struct + per-host
    // fingerprints under the same namespace.
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Settings {
      host: String,
      port: u16,
    }
    kv.set(
      "connect",
      Settings {
        host: "1.2.3.4".into(),
        port: 2222,
      },
    )
    .await
    .unwrap();
    kv.set("host_key:1.2.3.4:2222", "abc123").await.unwrap();

    assert_eq!(
      kv.get::<Settings>("connect").await.unwrap(),
      Some(Settings {
        host: "1.2.3.4".to_string(),
        port: 2222
      })
    );
    assert_eq!(kv.get::<String>("host_key:1.2.3.4:2222").await.unwrap(), Some("abc123".to_string()));
  });

  let stored = String::from_utf8(mock.storage_read("/apps/ssh/kv.json").unwrap()).unwrap();
  assert!(stored.contains("\"connect\""));
  assert!(stored.contains("\"host_key:1.2.3.4:2222\""));
}
