//! Persona Pack — portable persona schema for LLM applications.
//!
//! See `docs/persona-pack-spec.md` for the full spec.
//! Required fields are intentionally minimal: `[meta].id`, `[meta].name`, `[prompt].body`.
//! Everything else lives under `[extra.*]` and is not validated by this crate.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::macros::format_description;

/// A parsed Persona Pack (`prompt.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub meta: Meta,
    pub prompt: Prompt,
    /// Origin-reserved + custom namespaces. Untyped on purpose.
    #[serde(default)]
    pub extra: IndexMap<String, toml::Value>,
}

/// Default spec version assumed when `meta.spec_version` is omitted.
/// Forward-compat rule: missing = the schema as defined at v0.1.
pub const DEFAULT_SPEC_VERSION: &str = "0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub name: String,
    pub origin: String,
    #[serde(default)]
    pub short: Option<String>,
    /// Schema version this Pack was authored against, as `MAJOR.MINOR`
    /// (e.g. "0.1"). Optional — readers must treat a missing value as
    /// `DEFAULT_SPEC_VERSION`. Tools may use this to gate features.
    #[serde(default)]
    pub spec_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub body: String,
    #[serde(default)]
    pub synth: Option<String>,
}

/// Reserved origin tags. Anything else should use `custom:<tag>`.
pub const RESERVED_ORIGINS: &[&str] = &["gem", "claude", "skill", "orc", "hand"];

#[derive(Debug, Error)]
pub enum PersonaError {
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("empty required field: {0}")]
    EmptyField(&'static str),
    #[error("invalid origin: {0} (must be one of {RESERVED_ORIGINS:?} or `custom:<tag>`)")]
    InvalidOrigin(String),
    #[error("invalid id: {0} (must match [A-Za-z0-9_-]+)")]
    InvalidId(String),
    #[error("invalid spec_version: {0} (must match MAJOR.MINOR, e.g. \"0.1\")")]
    InvalidSpecVersion(String),
    #[error("persona not found: {0}")]
    NotFound(String),
}

impl Persona {
    /// Parse a `prompt.toml` string into a `Persona` and run minimum validation.
    pub fn from_toml_str(s: &str) -> Result<Self, PersonaError> {
        let p: Persona = toml::from_str(s)?;
        p.validate()?;
        Ok(p)
    }

    /// Minimum validation: required 3 fields exist & non-empty, origin is reserved or `custom:*`.
    /// `[extra.*]` is intentionally not inspected.
    pub fn validate(&self) -> Result<(), PersonaError> {
        if self.meta.id.is_empty() {
            return Err(PersonaError::EmptyField("meta.id"));
        }
        if !is_valid_id(&self.meta.id) {
            return Err(PersonaError::InvalidId(self.meta.id.clone()));
        }
        if self.meta.name.is_empty() {
            return Err(PersonaError::EmptyField("meta.name"));
        }
        if self.prompt.body.is_empty() {
            return Err(PersonaError::EmptyField("prompt.body"));
        }
        if !is_valid_origin(&self.meta.origin) {
            return Err(PersonaError::InvalidOrigin(self.meta.origin.clone()));
        }
        if let Some(v) = &self.meta.spec_version {
            if !is_valid_spec_version(v) {
                return Err(PersonaError::InvalidSpecVersion(v.clone()));
            }
        }
        Ok(())
    }

    /// Effective spec version: `meta.spec_version` if set, otherwise the
    /// `DEFAULT_SPEC_VERSION` ("0.1") that this reader was built against.
    pub fn effective_spec_version(&self) -> &str {
        self.meta
            .spec_version
            .as_deref()
            .unwrap_or(DEFAULT_SPEC_VERSION)
    }

    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

/// Filesystem helpers: a "Pack root" is a directory containing `<id>/prompt.toml` entries.
pub struct PackRoot {
    root: PathBuf,
}

impl PackRoot {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    fn pack_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn pack_toml(&self, id: &str) -> PathBuf {
        self.pack_dir(id).join("prompt.toml")
    }

    /// Read `<root>/<id>/prompt.toml` and validate.
    pub fn read(&self, id: &str) -> Result<Persona, PersonaError> {
        let path = self.pack_toml(id);
        if !path.exists() {
            return Err(PersonaError::NotFound(id.to_string()));
        }
        let s = std::fs::read_to_string(&path)?;
        Persona::from_toml_str(&s)
    }

    /// Write a Persona to `<root>/<id>/prompt.toml`.
    /// `id` is taken from `persona.meta.id`. The directory is created if missing.
    pub fn write(&self, persona: &Persona) -> Result<PathBuf, PersonaError> {
        persona.validate()?;
        let dir = self.pack_dir(&persona.meta.id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("prompt.toml");
        std::fs::write(&path, persona.to_toml_string()?)?;
        Ok(path)
    }

    /// Delete `<root>/<id>/prompt.toml`. Also remove `<root>/<id>/` if empty after.
    /// Returns the path that was removed. Error `PersonaError::NotFound` if missing.
    pub fn delete(&self, id: &str) -> Result<PathBuf, PersonaError> {
        let path = self.pack_toml(id);
        if !path.exists() {
            return Err(PersonaError::NotFound(id.to_string()));
        }
        std::fs::remove_file(&path)?;
        let dir = self.pack_dir(id);
        // Best-effort dir cleanup; ignore errors (dir may have other files)
        if std::fs::read_dir(&dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false)
        {
            let _ = std::fs::remove_dir(&dir);
        }
        Ok(path)
    }

    /// List Pack ids under the root (every subdir containing `prompt.toml`).
    pub fn list(&self) -> Result<Vec<String>, PersonaError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if entry.path().join("prompt.toml").exists() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Copy the current `<root>/<id>/prompt.toml` to `<root>/<id>/history/<UTC-ts>.toml`
    /// **before** any write operation overwrites it.
    ///
    /// # Arguments
    /// * `id` — Pack identifier
    ///
    /// # Returns
    /// * `Ok(Some(dst))` — snapshot was created at `dst`
    /// * `Ok(None)` — no existing `prompt.toml` (first write; skip is correct behaviour)
    /// * `Err(PersonaError::Io(...))` — I/O failure during directory creation or copy
    ///
    /// # Constraints (crux #1)
    /// This method only **copies** the existing file. It does not modify `prompt.toml`.
    /// The caller is responsible for calling `snapshot_before_write` before `write`.
    pub fn snapshot_before_write(&self, id: &str) -> Result<Option<PathBuf>, PersonaError> {
        let src = self.pack_toml(id);
        if !src.exists() {
            return Ok(None);
        }
        let history_dir = self.pack_dir(id).join("history");
        std::fs::create_dir_all(&history_dir)?;
        let fmt = format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]Z");
        // format_description! is validated at compile time; the only runtime failure
        // path is an I/O-level condition, so we map to PersonaError::Io.
        let ts = time::OffsetDateTime::now_utc()
            .format(&fmt)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let dst = history_dir.join(format!("{ts}.toml"));
        std::fs::copy(&src, &dst)?;
        Ok(Some(dst))
    }

    /// List historical snapshots for `<root>/<id>/history/*.toml` in **descending**
    /// timestamp order (most recent first).
    ///
    /// # Arguments
    /// * `id` — Pack identifier
    ///
    /// # Returns
    /// Each entry is `(timestamp_str, toml::Value)` where `timestamp_str` is the
    /// filename stem (e.g. `"2026-05-06T10-35-12Z"`) and `toml::Value` is the full
    /// parsed TOML document from that snapshot.  Returns `Ok(Vec::new())` when the
    /// `history/` directory does not exist (not an error; no snapshots yet).
    ///
    /// # Errors
    /// * `PersonaError::Io` — directory I/O failure
    /// * `PersonaError::Parse` — a snapshot file contains invalid TOML
    ///
    /// # Constraints (crux #3)
    /// Only `history/*.toml` files are included. `prompt.toml` is **never** added to
    /// the result, even when `at` is not supplied.
    pub fn history_list(&self, id: &str) -> Result<Vec<(String, toml::Value)>, PersonaError> {
        let history_dir = self.pack_dir(id).join("history");
        if !history_dir.exists() {
            return Ok(Vec::new());
        }
        let mut entries: Vec<(String, toml::Value)> = Vec::new();
        for entry in std::fs::read_dir(&history_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let content = std::fs::read_to_string(&path)?;
            let value: toml::Value = toml::from_str(&content)?;
            entries.push((stem.to_string(), value));
        }
        // Descending order: ISO8601-ish timestamp strings sort lexicographically = chronologically
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(entries)
    }

    /// Read a historical snapshot `<root>/<id>/history/<at>.toml` and parse it as a
    /// validated `Persona`.
    ///
    /// # Arguments
    /// * `id` — Pack identifier
    /// * `at` — timestamp string used as the filename stem (e.g. `"2026-05-06T10-35-12Z"`)
    ///
    /// # Returns
    /// The parsed and validated `Persona` from the requested snapshot.
    ///
    /// # Errors
    /// * `PersonaError::NotFound` — no snapshot file at `history/<at>.toml`
    /// * `PersonaError::Io` — file read failure
    /// * `PersonaError::Parse` — TOML parse error
    pub fn read_at(&self, id: &str, at: &str) -> Result<Persona, PersonaError> {
        let path = self.pack_dir(id).join("history").join(format!("{at}.toml"));
        if !path.exists() {
            return Err(PersonaError::NotFound(format!("{id}@{at}")));
        }
        let s = std::fs::read_to_string(&path)?;
        Persona::from_toml_str(&s)
    }
}

/// Walk a `toml::Value` tree using a dot-separated path (e.g. `"extra.version"`,
/// `"meta.name"`, `"prompt.body"`).
///
/// # Arguments
/// * `value` — root `toml::Value` to search (typically the full parsed TOML document)
/// * `path`  — dot-separated key path; array indices are **not** supported
///
/// # Returns
/// `Some(&toml::Value)` at the resolved path, or `None` if any segment is absent
/// or a non-table node is encountered mid-path.
///
/// # Constraints (crux #2)
/// This function must **never** restrict traversal to a fixed set of root keys.
/// All root keys (`extra`, `meta`, `prompt`, and any future key) are resolved
/// via the same uniform table-walk logic.
pub fn lookup_dot_path<'a>(value: &'a toml::Value, path: &str) -> Option<&'a toml::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.as_table()?.get(segment)?;
    }
    Some(current)
}

fn is_valid_origin(o: &str) -> bool {
    if RESERVED_ORIGINS.contains(&o) {
        return true;
    }
    if let Some(tag) = o.strip_prefix("custom:") {
        return !tag.is_empty() && tag.chars().all(is_id_char);
    }
    false
}

fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn is_valid_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_id_char)
}

fn is_valid_spec_version(s: &str) -> bool {
    let mut parts = s.split('.');
    let (Some(major), Some(minor), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !major.is_empty()
        && !minor.is_empty()
        && major.chars().all(|c| c.is_ascii_digit())
        && minor.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "You are Alice."
"#;

    #[test]
    fn parses_minimal() {
        let p = Persona::from_toml_str(MINIMAL).unwrap();
        assert_eq!(p.meta.id, "alice");
        assert_eq!(p.meta.name, "Alice");
        assert_eq!(p.meta.origin, "hand");
        assert_eq!(p.prompt.body, "You are Alice.");
    }

    #[test]
    fn rejects_empty_body() {
        let s = MINIMAL.replace("You are Alice.", "");
        let err = Persona::from_toml_str(&s).unwrap_err();
        assert!(matches!(err, PersonaError::EmptyField("prompt.body")));
    }

    #[test]
    fn rejects_unknown_origin() {
        let s = MINIMAL.replace("origin = \"hand\"", "origin = \"internal\"");
        let err = Persona::from_toml_str(&s).unwrap_err();
        assert!(matches!(err, PersonaError::InvalidOrigin(_)));
    }

    #[test]
    fn accepts_custom_origin() {
        let s = MINIMAL.replace("origin = \"hand\"", "origin = \"custom:internal\"");
        Persona::from_toml_str(&s).unwrap();
    }

    #[test]
    fn preserves_extra_namespace() {
        let s = format!("{MINIMAL}\n[extra.orc]\nrole = \"reviewer\"\n");
        let p = Persona::from_toml_str(&s).unwrap();
        assert!(p.extra.contains_key("orc"));
    }

    #[test]
    fn missing_spec_version_defaults_to_0_1() {
        let p = Persona::from_toml_str(MINIMAL).unwrap();
        assert_eq!(p.meta.spec_version, None);
        assert_eq!(p.effective_spec_version(), "0.1");
    }

    #[test]
    fn accepts_explicit_spec_version() {
        let s = MINIMAL.replace(
            "id     = \"alice\"",
            "spec_version = \"0.1\"\nid     = \"alice\"",
        );
        let p = Persona::from_toml_str(&s).unwrap();
        assert_eq!(p.meta.spec_version.as_deref(), Some("0.1"));
        assert_eq!(p.effective_spec_version(), "0.1");
    }

    #[test]
    fn rejects_malformed_spec_version() {
        let s = MINIMAL.replace(
            "id     = \"alice\"",
            "spec_version = \"v0.1\"\nid     = \"alice\"",
        );
        let err = Persona::from_toml_str(&s).unwrap_err();
        assert!(matches!(err, PersonaError::InvalidSpecVersion(_)));
    }

    #[test]
    fn rejects_traversal_id() {
        let s = MINIMAL.replace("id     = \"alice\"", "id     = \"../escape\"");
        let err = Persona::from_toml_str(&s).unwrap_err();
        assert!(matches!(err, PersonaError::InvalidId(_)));
    }

    #[test]
    fn rejects_slash_id() {
        let s = MINIMAL.replace("id     = \"alice\"", "id     = \"a/b\"");
        let err = Persona::from_toml_str(&s).unwrap_err();
        assert!(matches!(err, PersonaError::InvalidId(_)));
    }

    #[test]
    fn pack_root_round_trip() {
        let dir = tempdir_like();
        let root = PackRoot::new(&dir);
        let p = Persona::from_toml_str(MINIMAL).unwrap();
        let path = root.write(&p).unwrap();
        assert!(path.ends_with("alice/prompt.toml"));
        let loaded = root.read("alice").unwrap();
        assert_eq!(loaded.meta.id, "alice");
        let ids = root.list().unwrap();
        assert_eq!(ids, vec!["alice".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_root_read_missing() {
        let dir = tempdir_like();
        let root = PackRoot::new(&dir);
        let err = root.read("ghost").unwrap_err();
        assert!(matches!(err, PersonaError::NotFound(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Create a unique temporary directory for a single test. Using a per-call
    /// counter (via an atomic) avoids directory-name collisions when tests run
    /// in parallel within the same process (same `std::process::id()`).
    fn tempdir_like() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("persona-pack-test-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&p);
        // Safety: create_dir_all only fails on real I/O errors; test-only code.
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // ── lookup_dot_path tests ──────────────────────────────────────────────

    fn sample_toml_value() -> toml::Value {
        let s = r#"
[meta]
id   = "alice"
name = "Alice"

[prompt]
body = "You are Alice."

[extra]
version = "1.2.3"
"#;
        toml::from_str(s).unwrap()
    }

    #[test]
    fn lookup_dot_path_extra() {
        let v = sample_toml_value();
        let result = lookup_dot_path(&v, "extra.version");
        assert_eq!(result.and_then(|v| v.as_str()), Some("1.2.3"));
    }

    #[test]
    fn lookup_dot_path_meta() {
        let v = sample_toml_value();
        let result = lookup_dot_path(&v, "meta.name");
        assert_eq!(result.and_then(|v| v.as_str()), Some("Alice"));
    }

    #[test]
    fn lookup_dot_path_prompt() {
        let v = sample_toml_value();
        let result = lookup_dot_path(&v, "prompt.body");
        assert_eq!(result.and_then(|v| v.as_str()), Some("You are Alice."));
    }

    #[test]
    fn lookup_dot_path_missing() {
        let v = sample_toml_value();
        assert!(lookup_dot_path(&v, "extra.nonexistent").is_none());
        assert!(lookup_dot_path(&v, "no_such_root.key").is_none());
    }

    // ── snapshot_before_write tests ────────────────────────────────────────

    #[test]
    fn snapshot_before_write_skip_when_absent() {
        let dir = tempdir_like();
        let root = PackRoot::new(&dir);
        // No prompt.toml exists yet — first write scenario, expect Ok(None)
        let result = root.snapshot_before_write("alice").unwrap();
        assert!(result.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_before_write_creates_history() {
        let dir = tempdir_like();
        let root = PackRoot::new(&dir);
        // Write initial persona
        let p = Persona::from_toml_str(MINIMAL).unwrap();
        root.write(&p).unwrap();
        // Snapshot before second write
        let dst = root.snapshot_before_write("alice").unwrap();
        assert!(dst.is_some(), "snapshot should be created");
        let dst_path = dst.unwrap();
        assert!(dst_path.exists(), "snapshot file must exist on disk");
        // Confirm it's inside history/
        assert!(dst_path.parent().unwrap().ends_with("history"));
        // Confirm history_list now sees exactly 1 entry
        let history = root.history_list("alice").unwrap();
        assert_eq!(history.len(), 1, "exactly one snapshot expected");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── history_list tests ─────────────────────────────────────────────────

    #[test]
    fn history_list_returns_descending() {
        let dir = tempdir_like();
        let root = PackRoot::new(&dir);
        let p = Persona::from_toml_str(MINIMAL).unwrap();
        root.write(&p).unwrap();
        // First snapshot
        root.snapshot_before_write("alice").unwrap();
        // Sleep 1 second to guarantee distinct timestamps
        std::thread::sleep(std::time::Duration::from_secs(1));
        // Second snapshot
        root.snapshot_before_write("alice").unwrap();
        let history = root.history_list("alice").unwrap();
        assert_eq!(history.len(), 2, "two snapshots expected");
        // Verify descending order: first timestamp >= second timestamp
        assert!(
            history[0].0 >= history[1].0,
            "history must be in descending timestamp order"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_list_excludes_current_prompt_toml() {
        let dir = tempdir_like();
        let root = PackRoot::new(&dir);
        let p = Persona::from_toml_str(MINIMAL).unwrap();
        root.write(&p).unwrap();
        // Take one snapshot
        root.snapshot_before_write("alice").unwrap();
        let history = root.history_list("alice").unwrap();
        // Verify none of the entries has timestamp "prompt" (i.e., prompt.toml leaked in)
        for (ts, _) in &history {
            assert_ne!(ts, "prompt", "prompt.toml must not appear in history_list");
        }
        assert_eq!(history.len(), 1, "only the snapshot, not prompt.toml");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── read_at tests ──────────────────────────────────────────────────────

    #[test]
    fn read_at_returns_history_persona() {
        let dir = tempdir_like();
        let root = PackRoot::new(&dir);
        let p = Persona::from_toml_str(MINIMAL).unwrap();
        root.write(&p).unwrap();
        // Snapshot the initial write
        let dst = root.snapshot_before_write("alice").unwrap().unwrap();
        // Derive timestamp from snapshot filename stem
        let ts = dst
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        let loaded = root.read_at("alice", &ts).unwrap();
        assert_eq!(loaded.meta.id, "alice");
        assert_eq!(loaded.meta.name, "Alice");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_at_missing_returns_not_found() {
        let dir = tempdir_like();
        let root = PackRoot::new(&dir);
        let err = root.read_at("alice", "2000-01-01T00-00-00Z").unwrap_err();
        assert!(
            matches!(err, PersonaError::NotFound(_)),
            "missing snapshot must yield NotFound"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
