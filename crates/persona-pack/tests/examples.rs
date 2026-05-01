//! Smoke test: every Pack under `examples/` parses and validates.

use std::fs;
use std::path::PathBuf;

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
