//! Smoke test: every Pack under `examples/` parses and validates.

use std::fs;
use std::path::PathBuf;

use persona_pack::{PackRoot, Persona, PersonaError};

#[test]
fn all_examples_parse() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples");

    let mut count = 0;
    for entry in fs::read_dir(&root).expect("examples/ exists") {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let toml_path = entry.path().join("prompt.toml");
        if !toml_path.exists() {
            continue;
        }
        let s = fs::read_to_string(&toml_path).unwrap();
        let p = persona_pack::Persona::from_toml_str(&s)
            .unwrap_or_else(|e| panic!("{} failed: {e}", toml_path.display()));
        assert!(!p.meta.id.is_empty());
        count += 1;
    }
    assert!(count >= 2, "expected at least alice + bob, got {count}");
}

const MINIMAL_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "You are Alice."
"#;

fn tempdir_unique(suffix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "persona-pack-test-{}-{}",
        std::process::id(),
        suffix
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// write → delete → list: persona disappears. Second delete returns NotFound.
#[test]
fn pack_root_delete_removes_and_idempotent() {
    let dir = tempdir_unique("del");
    let root = PackRoot::new(&dir);

    let persona = Persona::from_toml_str(MINIMAL_TOML).unwrap();
    root.write(&persona).unwrap();

    // sanity: appears in list
    assert_eq!(root.list().unwrap(), vec!["alice".to_string()]);

    // delete succeeds and returns the removed path
    let removed = root.delete("alice").unwrap();
    assert!(removed.ends_with("alice/prompt.toml"));

    // no longer in list
    assert!(root.list().unwrap().is_empty());

    // second delete → NotFound
    let err = root.delete("alice").unwrap_err();
    assert!(
        matches!(err, PersonaError::NotFound(_)),
        "expected NotFound, got: {err}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// When the dir has extra files after deleting prompt.toml, the dir is kept.
#[test]
fn pack_root_delete_keeps_dir_if_non_empty() {
    let dir = tempdir_unique("del2");
    let root = PackRoot::new(&dir);

    let persona = Persona::from_toml_str(MINIMAL_TOML).unwrap();
    root.write(&persona).unwrap();

    // plant an extra file in alice/ so the dir is not empty after deletion
    let extra = dir.join("alice").join("extra.txt");
    std::fs::write(&extra, "keep me").unwrap();

    root.delete("alice").unwrap();

    // dir must still exist (not cleaned up) because extra.txt is there
    assert!(
        dir.join("alice").exists(),
        "alice/ should survive when non-empty"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
