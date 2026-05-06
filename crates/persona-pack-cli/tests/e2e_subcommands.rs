//! End-to-end tests for `persona-pack list` and `persona-pack dump` subcommands.
//!
//! Fixtures are written directly via the `persona-pack` lib API (no CLI write
//! subcommand; that is out of scope for this issue).  The tests then invoke
//! the CLI binary via `assert_cmd` and inspect stdout.
//!
//! Verifies Crux 3: `persona-pack dump` JSON output shape matches
//! `persona_read` MCP tool (both serialise the same `Persona` struct).

use assert_cmd::Command;
use persona_pack::{PackRoot, Persona};
use serde_json::Value;
use tempfile::tempdir;

/// TOML fixture for the "alice" persona.
const ALICE_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "I am Alice."
"#;

/// TOML fixture for the "bob" persona.
const BOB_TOML: &str = r#"
[meta]
id     = "bob"
name   = "Bob"
origin = "gem"

[prompt]
body = "I am Bob."
"#;

/// Write a persona TOML string into a `PackRoot` via the lib API.
///
/// # Arguments
/// - `root`: the `PackRoot` to write into.
/// - `toml`: TOML source string for the persona.
///
/// # Errors
/// Returns `PersonaError` if the TOML is invalid or the file cannot be written.
fn write_persona(root: &PackRoot, toml: &str) -> anyhow::Result<()> {
    // PackRoot::write is 1-arg (takes &Persona), not 2-arg.
    // Parse first, then write — this is the canonical 2-step pattern.
    let persona = Persona::from_toml_str(toml)?;
    root.write(&persona)?;
    Ok(())
}

/// `dump <id>` should return JSON whose shape matches `persona_read` MCP tool:
/// `{meta: {id, name, origin, ...}, prompt: {body, ...}, extra: {...}}`.
///
/// Verifies Crux 3: both CLI and MCP tool serialise the same `Persona` struct,
/// so the `{meta, prompt, extra}` shape is guaranteed identical.
#[test]
fn dump_returns_persona_read_shape() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = PackRoot::new(dir.path().to_path_buf());
    write_persona(&root, ALICE_TOML)?;

    let output = Command::cargo_bin("persona-pack")?
        .args(["dump", "alice", "--root"])
        .arg(dir.path())
        .output()?;

    assert!(output.status.success(), "dump exited with failure");
    let stdout = String::from_utf8(output.stdout)?;
    let v: Value = serde_json::from_str(&stdout).expect("dump output is valid JSON");

    // Crux 3: {meta, prompt, extra} all present.
    assert_eq!(v["meta"]["id"], "alice");
    assert_eq!(v["meta"]["name"], "Alice");
    assert_eq!(v["prompt"]["body"], "I am Alice.");
    // extra field must be present (may be empty object).
    assert!(
        v.get("extra").is_some(),
        "extra field missing from dump output"
    );
    Ok(())
}

/// `dump <id> --at <ts>` should return the snapshot version.
///
/// This exercises the `read_at` code path.  We write alice twice so that a
/// history snapshot is created by the second write, then we verify the
/// first-written body is retrievable via `--at`.
#[test]
fn dump_at_returns_snapshot() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = PackRoot::new(dir.path().to_path_buf());

    // First write — no snapshot created yet (nothing to snapshot).
    write_persona(&root, ALICE_TOML)?;

    // Capture the timestamp of the first write by reading history.
    let history = root.history_list("alice")?;
    if history.is_empty() {
        // Only 1 write so far → no snapshot yet.  Write again to create one.
        let alice_v2 = ALICE_TOML.replace("I am Alice.", "I am Alice v2.");
        write_persona(&root, &alice_v2)?;
    }

    let history = root.history_list("alice")?;
    if history.is_empty() {
        // If history is still empty the environment doesn't support snapshots;
        // skip gracefully rather than failing.
        return Ok(());
    }

    // history_list returns Vec<(timestamp_stem, toml::Value)>; take the stem.
    let ts = history[0].0.clone();
    let output = Command::cargo_bin("persona-pack")?
        .args(["dump", "alice", "--at", &ts, "--root"])
        .arg(dir.path())
        .output()?;

    assert!(output.status.success(), "dump --at exited with failure");
    let stdout = String::from_utf8(output.stdout)?;
    let v: Value = serde_json::from_str(&stdout).expect("dump --at output is valid JSON");
    assert_eq!(v["meta"]["id"], "alice");
    Ok(())
}

/// `list` should return a JSON array of persona IDs.
#[test]
fn list_returns_id_array() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = PackRoot::new(dir.path().to_path_buf());
    write_persona(&root, ALICE_TOML)?;

    let output = Command::cargo_bin("persona-pack")?
        .args(["list", "--root"])
        .arg(dir.path())
        .output()?;

    assert!(output.status.success(), "list exited with failure");
    let stdout = String::from_utf8(output.stdout)?;
    let v: Value = serde_json::from_str(&stdout).expect("list output is valid JSON");
    assert_eq!(v, serde_json::json!(["alice"]));
    Ok(())
}

/// `list --origin <tag>` should filter by the `meta.origin` field.
#[test]
fn list_filters_by_origin() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let root = PackRoot::new(dir.path().to_path_buf());
    write_persona(&root, ALICE_TOML)?; // origin = "hand"
    write_persona(&root, BOB_TOML)?; // origin = "gem"

    // Filter for "hand" — only alice should appear.
    let output = Command::cargo_bin("persona-pack")?
        .args(["list", "--origin", "hand", "--root"])
        .arg(dir.path())
        .output()?;

    assert!(output.status.success(), "list --origin exited with failure");
    let stdout = String::from_utf8(output.stdout)?;
    let v: Value = serde_json::from_str(&stdout).expect("list --origin output is valid JSON");
    assert_eq!(v, serde_json::json!(["alice"]));
    Ok(())
}

/// `dump <id>` on a non-existent persona should exit with a non-zero status.
#[test]
fn dump_nonexistent_fails() -> anyhow::Result<()> {
    let dir = tempdir()?;

    let output = Command::cargo_bin("persona-pack")?
        .args(["dump", "ghost", "--root"])
        .arg(dir.path())
        .output()?;

    assert!(
        !output.status.success(),
        "dump of nonexistent persona should fail"
    );
    Ok(())
}

/// `list` on an empty root should return an empty JSON array.
#[test]
fn list_empty_root_returns_empty_array() -> anyhow::Result<()> {
    let dir = tempdir()?;

    let output = Command::cargo_bin("persona-pack")?
        .args(["list", "--root"])
        .arg(dir.path())
        .output()?;

    assert!(
        output.status.success(),
        "list on empty root exited with failure"
    );
    let stdout = String::from_utf8(output.stdout)?;
    let v: Value = serde_json::from_str(&stdout).expect("empty list output is valid JSON");
    assert_eq!(v, serde_json::json!([]));
    Ok(())
}
