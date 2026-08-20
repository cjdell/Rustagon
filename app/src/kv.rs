//! Per-app key-value storage: one JSON file per namespace, for app settings
//! and state that must survive relaunch (SSH's connect form + host-key
//! fingerprints, future per-app preferences).
//!
//! # Layout
//!
//! Each namespace is a single JSON object file at `/apps/<namespace>/kv.json`
//! mapping string keys to arbitrary serde-serialised values:
//!
//! ```json
//! { "connect": { "host": "192.168.49.1", "user": "cjdell", "key": "id_ed255.key", "port": 22 },
//!   "host_key:192.168.49.1:22": "3f5e..." }
//! ```
//!
//! Obtain a namespace via [`MenuAppContext::kv`](crate::apps::MenuAppContext::kv):
//!
//! ```rust,ignore
//! let kv = self.ctx.kv("ssh");
//! kv.set("connect", settings).await?;
//! let settings: Option<SshSettings> = kv.get("connect").await?;
//! ```
//!
//! # Atomicity
//!
//! [`LocalFsTrait`](crate::platform::LocalFsTrait) has no `rename`, so a
//! write-tmp-then-rename swap is not possible on either platform. The
//! documented fallback: the whole file is rewritten in a single
//! truncate-and-write (`write_text_file`), serialised by the platform's
//! filesystem mutex. A power loss mid-write can therefore leave a
//! namespace file torn; `load` tolerates that — an unreadable or
//! non-JSON file is treated as *empty* (and logged) rather than as an
//! error, so a torn KV file costs the app its cached settings but never
//! blocks it.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use log::{debug, warn};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::value::{Map, Value};

use crate::platform::storage::{FsError, StorageHandle};

/// An error from a KV operation.
#[derive(Debug, Clone, PartialEq)]
pub enum KvError {
  /// A storage operation failed.
  Storage(String),
  /// The namespace file exists but is not valid JSON (torn write, hand
  /// edit). Treat the namespace as empty.
  Corrupt,
  /// A value stored under the key does not decode to the requested type.
  Decode(String),
}

impl KvError {
  pub fn to_display(&self) -> &str {
    match self {
      KvError::Storage(msg) => msg,
      KvError::Corrupt => "Stored data is corrupt",
      KvError::Decode(msg) => msg,
    }
  }
}

impl core::fmt::Display for KvError {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str(self.to_display())
  }
}

/// The per-namespace store. Cheap to construct and clone (it owns no state
/// — every operation re-reads/rewrites the file, which the filesystem
/// serialises).
#[derive(Clone, Debug)]
pub struct KvNamespace {
  storage: StorageHandle,
  /// `/apps/<namespace>` — created (best-effort) before writes.
  parent: String,
  /// `/apps/<namespace>/kv.json`.
  path: String,
}

impl KvNamespace {
  pub fn new(storage: StorageHandle, namespace: &str) -> Self {
    let parent = alloc::format!("/apps/{namespace}");
    Self {
      storage,
      path: alloc::format!("{parent}/kv.json"),
      parent,
    }
  }

  /// Load the namespace map. A missing file yields an empty map; a corrupt
  /// file yields an empty map (with a log warning) — see the module docs.
  async fn load(&self) -> BTreeMap<String, Value> {
    match self.storage.read_text_file(self.path.clone()).await {
      Ok(text) => match serde_json::from_str::<Map<String, Value>>(&text) {
        Ok(map) => map.into_iter().collect(),
        Err(err) => {
          warn!("kv: {} is corrupt ({err}), starting empty", self.path);
          BTreeMap::new()
        }
      },
      Err(FsError::NotFound) => BTreeMap::new(),
      Err(err) => {
        warn!("kv: reading {} failed: {err:?}", self.path);
        BTreeMap::new()
      }
    }
  }

  /// Rewrite the whole namespace file (single truncate-and-write; the
  /// filesystem mutex serialises writers — see the module docs).
  async fn save(&self, map: &BTreeMap<String, Value>) -> Result<(), KvError> {
    // littlefs `open(CREATE)` does not create parent directories.
    if let Err(err) = self.storage.mkdir(self.parent.clone()).await && !matches!(err, FsError::AlreadyExists) {
      return Err(KvError::Storage(alloc::format!("mkdir {}: {err:?}", self.parent)));
    }
    let text = serde_json::to_string(map).map_err(|err| KvError::Decode(err.to_string()))?;
    self
      .storage
      .write_text_file(self.path.clone(), text)
      .await
      .map_err(|err| KvError::Storage(alloc::format!("write {}: {err:?}", self.path)))
  }

  /// Read the value stored under `key`, deserialising it to `T`.
  /// `Ok(None)` when the key is absent (or the namespace is missing/corrupt).
  pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, KvError> {
    let map = self.load().await;
    match map.get(key) {
      None => Ok(None),
      Some(value) => T::deserialize(value.clone()).map(Some).map_err(|err| KvError::Decode(err.to_string()))
    }
  }

  /// Store `value` under `key` (replacing any existing value).
  pub async fn set<T: Serialize>(&self, key: &str, value: T) -> Result<(), KvError> {
    let value = serde_json::to_value(&value).map_err(|err| KvError::Decode(err.to_string()))?;
    let mut map = self.load().await;
    map.insert(key.to_string(), value);
    debug!("kv: set {}/{} ({} keys)", self.path, key, map.len());
    self.save(&map).await
  }

  /// Remove `key`. Absent keys are not an error.
  pub async fn remove(&self, key: &str) -> Result<(), KvError> {
    let mut map = self.load().await;
    if map.remove(key).is_some() {
      self.save(&map).await
    } else {
      Ok(())
    }
  }

  /// True when `key` has a value stored.
  pub async fn contains(&self, key: &str) -> bool {
    self.load().await.contains_key(key)
  }
}

// The async round-trip tests need an executor + an in-memory `LocalFsTrait`,
// which only exist in the test harness; they live in `tests/kv.rs`
// (integration, std) on top of `app::testing::MockStorage`.
