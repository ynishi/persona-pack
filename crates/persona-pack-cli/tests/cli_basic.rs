use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

const ALICE_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "You are Alice."
"#;

const BOB_TOML: &str = r#"
[meta]
id     = "bob"
name   = "Bob"
origin = "gem"

[prompt]
body = "You are Bob."
"#;

fn write_persona(root: &TempDir, toml: &str, id: &str) {
    let dir = root.path().join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("prompt.toml"), toml).unwrap();
}

fn persona_pack_cmd() -> Command {
    Command::cargo_bin("persona-pack").unwrap()
}

// ── list ─────────────────────────────────────────────────────────────────────

#[test]
fn list_empty_root_returns_empty_array() {
    let root = TempDir::new().unwrap();
    let output = persona_pack_cmd()
        .args(["list", "--root", root.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let ids: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
    assert!(ids.is_empty());
}

#[test]
fn list_one_pack_returns_id() {
    let root = TempDir::new().unwrap();
    write_persona(&root, ALICE_TOML, "alice");

    let output = persona_pack_cmd()
        .args(["list", "--root", root.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let ids: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(ids, vec!["alice".to_string()]);
}

#[test]
fn list_two_packs_returns_sorted_ids() {
    let root = TempDir::new().unwrap();
    write_persona(&root, ALICE_TOML, "alice");
    write_persona(&root, BOB_TOML, "bob");

    let output = persona_pack_cmd()
        .args(["list", "--root", root.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let ids: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(ids, vec!["alice".to_string(), "bob".to_string()]);
}

#[test]
fn list_origin_filter_hand_returns_only_alice() {
    let root = TempDir::new().unwrap();
    write_persona(&root, ALICE_TOML, "alice"); // origin = hand
    write_persona(&root, BOB_TOML, "bob"); // origin = gem

    let output = persona_pack_cmd()
        .args([
            "list",
            "--root",
            root.path().to_str().unwrap(),
            "--origin",
            "hand",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let ids: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(ids, vec!["alice".to_string()]);
}

#[test]
fn list_origin_filter_gem_returns_only_bob() {
    let root = TempDir::new().unwrap();
    write_persona(&root, ALICE_TOML, "alice"); // origin = hand
    write_persona(&root, BOB_TOML, "bob"); // origin = gem

    let output = persona_pack_cmd()
        .args([
            "list",
            "--root",
            root.path().to_str().unwrap(),
            "--origin",
            "gem",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let ids: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(ids, vec!["bob".to_string()]);
}

#[test]
fn list_origin_filter_no_match_returns_empty() {
    let root = TempDir::new().unwrap();
    write_persona(&root, ALICE_TOML, "alice");

    let output = persona_pack_cmd()
        .args([
            "list",
            "--root",
            root.path().to_str().unwrap(),
            "--origin",
            "claude",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let ids: Vec<String> = serde_json::from_str(stdout.trim()).unwrap();
    assert!(ids.is_empty());
}

// ── dump ─────────────────────────────────────────────────────────────────────

/// Crux 3: dump output must include {meta, prompt, extra} fields matching Persona shape.
#[test]
fn dump_returns_full_persona_shape() {
    let root = TempDir::new().unwrap();
    write_persona(&root, ALICE_TOML, "alice");

    let output = persona_pack_cmd()
        .args(["dump", "alice", "--root", root.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();

    // All top-level fields present (meta, prompt, extra)
    assert!(v.get("meta").is_some(), "meta field missing");
    assert!(v.get("prompt").is_some(), "prompt field missing");
    assert!(v.get("extra").is_some(), "extra field missing");

    // meta.id matches (Crux 3 shape verification)
    assert_eq!(v["meta"]["id"], "alice");
    assert_eq!(v["meta"]["name"], "Alice");
    assert_eq!(v["meta"]["origin"], "hand");
    assert_eq!(v["prompt"]["body"], "You are Alice.");
}

#[test]
fn dump_missing_persona_exits_nonzero() {
    let root = TempDir::new().unwrap();

    let output = persona_pack_cmd()
        .args([
            "dump",
            "nonexistent",
            "--root",
            root.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

// ── help / subcommands visible ────────────────────────────────────────────────

#[test]
fn help_shows_all_subcommands() {
    let output = persona_pack_cmd().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("list"), "list missing from help");
    assert!(stdout.contains("dump"), "dump missing from help");
    assert!(stdout.contains("mcp"), "mcp missing from help");
}
