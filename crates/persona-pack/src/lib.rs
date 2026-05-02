//! Persona Pack — portable persona schema for LLM applications.
//!
//! See `docs/persona-pack-spec.md` for the full spec.
//! Required fields are intentionally minimal: `[meta].id`, `[meta].name`, `[prompt].body`.
//! Everything else lives under `[extra.*]` and is not validated by this crate.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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

    fn tempdir_like() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("persona-pack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
