//! End-to-end smoke test: spawn the stdio server and exercise all 4 tools.

use std::borrow::Cow;

use rmcp::{
    model::{CallToolRequestParams, ReadResourceRequestParams},
    transport::TokioChildProcess,
    ServiceExt,
};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::time::{sleep, Duration};

fn server_command() -> Command {
    // persona-pack-mcp binary was replaced by `persona-pack mcp` (CLI crate).
    let bin = std::env::var("CARGO_BIN_EXE_persona-pack").unwrap_or_else(|_| {
        format!(
            "{}/../../target/debug/persona-pack",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    let mut cmd = Command::new(bin);
    cmd.arg("mcp");
    cmd
}

fn call_params(name: &str, args: Value) -> CallToolRequestParams {
    let arguments = match args {
        Value::Object(m) => Some(m),
        _ => None,
    };
    let mut p = CallToolRequestParams::default();
    p.name = Cow::Owned(name.to_string());
    p.arguments = arguments;
    p
}

fn extract_text(result: &rmcp::model::CallToolResult) -> &str {
    result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("")
}

const ALICE_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "You are Alice."
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_crud_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // list_tools
    let tools = client.list_all_tools().await.expect("list tools");
    let names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "persona_write",
        "persona_read",
        "persona_render",
        "persona_list",
        "persona_validate",
        "persona_info",
    ] {
        assert!(names.contains(&expected), "missing tool: {expected}");
    }

    // info should report the configured default root
    let info = client
        .call_tool(call_params("persona_info", json!({})))
        .await
        .expect("info call");
    let info_text = extract_text(&info);
    assert!(info_text.contains("\"root\""), "got: {info_text}");
    assert!(info_text.contains("\"version\""), "got: {info_text}");
    assert!(info_text.contains("\"persona_count\""), "got: {info_text}");

    // write
    let write_res = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": ALICE_TOML, "root": root }),
        ))
        .await
        .expect("write call");
    assert!(!write_res.is_error.unwrap_or(false));

    // validate
    let v = client
        .call_tool(call_params(
            "persona_validate",
            json!({ "id": "alice", "root": root }),
        ))
        .await
        .expect("validate call");
    let v_text = extract_text(&v);
    assert!(v_text.contains("\"ok\":true"), "got: {v_text}");

    // read
    let r = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "root": root }),
        ))
        .await
        .expect("read call");
    let r_text = extract_text(&r);
    assert!(r_text.contains("\"id\":\"alice\""), "got: {r_text}");

    // render: default = "prompt" → just the body text, no JSON wrapping
    let rp = client
        .call_tool(call_params(
            "persona_render",
            json!({ "id": "alice", "root": root }),
        ))
        .await
        .expect("render(prompt) call");
    let rp_text = extract_text(&rp);
    assert_eq!(rp_text, "You are Alice.");

    // render: format = "header" → meta header + body
    let rh = client
        .call_tool(call_params(
            "persona_render",
            json!({ "id": "alice", "root": root, "format": "header" }),
        ))
        .await
        .expect("render(header) call");
    let rh_text = extract_text(&rh);
    assert!(rh_text.starts_with("# Alice  (origin: hand)"));
    assert!(rh_text.contains("You are Alice."));

    // render: format = "json" → equivalent to persona_read
    let rj = client
        .call_tool(call_params(
            "persona_render",
            json!({ "id": "alice", "root": root, "format": "json" }),
        ))
        .await
        .expect("render(json) call");
    let rj_text = extract_text(&rj);
    assert!(rj_text.contains("\"id\":\"alice\""));

    // render: unknown format → error
    let re = client
        .call_tool(call_params(
            "persona_render",
            json!({ "id": "alice", "root": root, "format": "bogus" }),
        ))
        .await
        .expect("render(bogus) call");
    assert!(re.is_error.unwrap_or(false), "expected error result");

    // list
    let l = client
        .call_tool(call_params("persona_list", json!({ "root": root })))
        .await
        .expect("list call");
    let l_text = extract_text(&l);
    assert!(l_text.contains("\"alice\""), "got: {l_text}");

    // list with origin filter that excludes alice
    let l2 = client
        .call_tool(call_params(
            "persona_list",
            json!({ "root": root, "origin": "gem" }),
        ))
        .await
        .expect("list call (filtered)");
    let l2_text = extract_text(&l2);
    assert!(
        l2_text.contains("\"personas\":[]"),
        "expected empty, got: {l2_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // persona_delete is listed as a tool
    let tools = client.list_all_tools().await.expect("list tools");
    let names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(
        names.contains(&"persona_delete"),
        "missing tool: persona_delete"
    );

    // write alice
    let write_res = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": ALICE_TOML, "root": root }),
        ))
        .await
        .expect("write call");
    assert!(!write_res.is_error.unwrap_or(false));

    // delete alice → ok
    let del_res = client
        .call_tool(call_params(
            "persona_delete",
            json!({ "id": "alice", "root": root }),
        ))
        .await
        .expect("delete call");
    assert!(!del_res.is_error.unwrap_or(false));
    let del_text = extract_text(&del_res);
    assert!(del_text.contains("\"ok\":true"), "got: {del_text}");

    // read alice after delete → error (NotFound)
    let read_res = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "root": root }),
        ))
        .await
        .expect("read call");
    assert!(
        read_res.is_error.unwrap_or(false),
        "expected error after delete, got: {}",
        extract_text(&read_res)
    );

    client.cancel().await.unwrap();
}

const ALICE_V1: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "You are Alice v1."

[extra]
version = "1.0"
"#;

const ALICE_V2: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "You are Alice v2."

[extra]
version = "2.0"
"#;

const ALICE_V3: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "You are Alice v3."

[extra]
version = "3.0"
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // persona_history is listed as a tool
    let tools = client.list_all_tools().await.expect("list tools");
    let names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(
        names.contains(&"persona_history"),
        "missing tool: persona_history"
    );

    // write1 (ALICE_V1) — first write, no snapshot yet
    let w1 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": ALICE_V1, "root": root }),
        ))
        .await
        .expect("write1 call");
    assert!(!w1.is_error.unwrap_or(false), "write1 failed");

    // timestamp collision guard: 1-second granularity requires a gap
    sleep(Duration::from_secs(1)).await;

    // write2 (ALICE_V2) — snapshot of ALICE_V1 is written to history/ before overwrite
    let w2 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": ALICE_V2, "root": root }),
        ))
        .await
        .expect("write2 call");
    assert!(!w2.is_error.unwrap_or(false), "write2 failed");

    // crux #1: history[0].value must be "1.0" (write1 content), not "2.0"
    // If snapshot happened after overwrite, this would return "2.0" and fail.
    let h1 = client
        .call_tool(call_params(
            "persona_history",
            json!({ "id": "alice", "view": "extra.version", "root": root }),
        ))
        .await
        .expect("history call after write2");
    assert!(!h1.is_error.unwrap_or(false), "history call failed");
    let h1_text = extract_text(&h1);
    let h1_items: Vec<Value> = serde_json::from_str(h1_text)
        .unwrap_or_else(|e| panic!("parse history json: {e}\ngot: {h1_text}"));

    // crux #3: write 2 times → history len = 1 (first write has no prior snapshot)
    assert_eq!(
        h1_items.len(),
        1,
        "expected 1 history entry after 2 writes (current prompt.toml excluded), got: {h1_text}"
    );

    // crux #1 continued: the single snapshot must contain v1 content
    assert_eq!(
        h1_items[0]["value"], "1.0",
        "history[0].value must be write1 content (1.0), got: {h1_text}"
    );

    // crux #2: dot-path works across different root keys — meta.name
    let h1_meta = client
        .call_tool(call_params(
            "persona_history",
            json!({ "id": "alice", "view": "meta.name", "root": root }),
        ))
        .await
        .expect("history meta.name call");
    assert!(
        !h1_meta.is_error.unwrap_or(false),
        "history meta.name failed"
    );
    let h1_meta_text = extract_text(&h1_meta);
    let h1_meta_items: Vec<Value> = serde_json::from_str(h1_meta_text)
        .unwrap_or_else(|e| panic!("parse history meta.name json: {e}\ngot: {h1_meta_text}"));
    assert_eq!(h1_meta_items.len(), 1);
    assert_eq!(
        h1_meta_items[0]["value"], "Alice",
        "meta.name view should return Alice, got: {h1_meta_text}"
    );

    // capture timestamp of the single history entry (write1 snapshot)
    let ts1 = h1_items[0]["timestamp"]
        .as_str()
        .expect("timestamp must be string")
        .to_string();

    sleep(Duration::from_secs(1)).await;

    // write3 (ALICE_V3) — snapshot of ALICE_V2 is added to history/
    let w3 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": ALICE_V3, "root": root }),
        ))
        .await
        .expect("write3 call");
    assert!(!w3.is_error.unwrap_or(false), "write3 failed");

    // crux #3: write 3 times → history len = 2, descending order (idx0 = write2, idx1 = write1)
    let h2 = client
        .call_tool(call_params(
            "persona_history",
            json!({ "id": "alice", "view": "extra.version", "root": root }),
        ))
        .await
        .expect("history call after write3");
    assert!(
        !h2.is_error.unwrap_or(false),
        "history call after write3 failed"
    );
    let h2_text = extract_text(&h2);
    let h2_items: Vec<Value> = serde_json::from_str(h2_text)
        .unwrap_or_else(|e| panic!("parse history json: {e}\ngot: {h2_text}"));

    assert_eq!(
        h2_items.len(),
        2,
        "expected 2 history entries after 3 writes, got: {h2_text}"
    );
    // descending order: idx0 = write2 snapshot (2.0), idx1 = write1 snapshot (1.0)
    assert_eq!(
        h2_items[0]["value"], "2.0",
        "idx0 must be write2 content (2.0), got: {h2_text}"
    );
    assert_eq!(
        h2_items[1]["value"], "1.0",
        "idx1 must be write1 content (1.0), got: {h2_text}"
    );

    // crux #2 continued: prompt.body view also works
    let h2_body = client
        .call_tool(call_params(
            "persona_history",
            json!({ "id": "alice", "view": "prompt.body", "root": root }),
        ))
        .await
        .expect("history prompt.body call");
    assert!(
        !h2_body.is_error.unwrap_or(false),
        "history prompt.body failed"
    );
    let h2_body_text = extract_text(&h2_body);
    let h2_body_items: Vec<Value> = serde_json::from_str(h2_body_text)
        .unwrap_or_else(|e| panic!("parse history prompt.body json: {e}\ngot: {h2_body_text}"));
    assert_eq!(h2_body_items.len(), 2);
    assert_eq!(
        h2_body_items[0]["value"], "You are Alice v2.",
        "prompt.body idx0 must be v2, got: {h2_body_text}"
    );
    assert_eq!(
        h2_body_items[1]["value"], "You are Alice v1.",
        "prompt.body idx1 must be v1, got: {h2_body_text}"
    );

    // persona_read with at=ts1 should return write1 content (version == "1.0")
    let rat = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "at": ts1, "root": root }),
        ))
        .await
        .expect("read at ts1 call");
    assert!(!rat.is_error.unwrap_or(false), "read at ts1 failed");
    let rat_text = extract_text(&rat);
    assert!(
        rat_text.contains("\"version\":\"1.0\"") || rat_text.contains("\"version\": \"1.0\""),
        "persona_read(at=ts1) must return v1 content, got: {rat_text}"
    );

    client.cancel().await.unwrap();
}

// ── private_fields e2e tests ──────────────────────────────────────────────────

/// TOML with a private extra field.
const PRIVATE_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"
private_fields = ["extra.secret"]

[prompt]
body = "You are Alice."

[extra]
secret = "mysecret"
public = "visible"
"#;

/// TOML with no private fields (for first-write tests).
const PUBLIC_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"

[prompt]
body = "You are Alice."

[extra]
public = "visible"
"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_with_as_self_returns_full() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // Write with as=alice (owner)
    let w = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write call");
    assert!(
        !w.is_error.unwrap_or(false),
        "write failed: {}",
        extract_text(&w)
    );

    // Read with as=alice → full persona including extra.secret
    let r = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "root": root, "as": "alice" }),
        ))
        .await
        .expect("read call");
    assert!(!r.is_error.unwrap_or(false));
    let r_text = extract_text(&r);
    assert!(
        r_text.contains("\"secret\""),
        "owner should see private field; got: {r_text}"
    );
    assert!(
        r_text.contains("mysecret"),
        "owner should see private value; got: {r_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_with_as_other_strips_private() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    let w = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write call");
    assert!(!w.is_error.unwrap_or(false));

    // Read with as=other → secret key stripped entirely
    let r = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "root": root, "as": "other_id" }),
        ))
        .await
        .expect("read call");
    assert!(!r.is_error.unwrap_or(false));
    let r_text = extract_text(&r);
    assert!(
        !r_text.contains("\"secret\""),
        "non-owner must not see private key; got: {r_text}"
    );
    assert!(
        !r_text.contains("mysecret"),
        "non-owner must not see private value; got: {r_text}"
    );
    // public field still visible
    assert!(
        r_text.contains("\"public\""),
        "public field must remain; got: {r_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_with_as_omitted_strips_private() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    let w = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write call");
    assert!(!w.is_error.unwrap_or(false));

    // Read with as omitted → anonymous = stripped
    let r = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "root": root }),
        ))
        .await
        .expect("read call");
    assert!(!r.is_error.unwrap_or(false));
    let r_text = extract_text(&r);
    assert!(
        !r_text.contains("\"secret\""),
        "anonymous must not see private key; got: {r_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn render_with_as_other_strips_in_json_format() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    let w = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write call");
    assert!(!w.is_error.unwrap_or(false));

    // render json format with as=other → secret stripped
    let r = client
        .call_tool(call_params(
            "persona_render",
            json!({ "id": "alice", "root": root, "format": "json", "as": "other_id" }),
        ))
        .await
        .expect("render call");
    assert!(!r.is_error.unwrap_or(false));
    let r_text = extract_text(&r);
    assert!(
        !r_text.contains("\"secret\""),
        "non-owner render json must not expose secret key; got: {r_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn render_prompt_format_unaffected_by_as() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    let w = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write call");
    assert!(!w.is_error.unwrap_or(false));

    // render prompt format: body is always present regardless of as
    let r_other = client
        .call_tool(call_params(
            "persona_render",
            json!({ "id": "alice", "root": root, "format": "prompt", "as": "other_id" }),
        ))
        .await
        .expect("render prompt other call");
    assert!(!r_other.is_error.unwrap_or(false));
    let body_other = extract_text(&r_other).to_string();

    let r_self = client
        .call_tool(call_params(
            "persona_render",
            json!({ "id": "alice", "root": root, "format": "prompt", "as": "alice" }),
        ))
        .await
        .expect("render prompt self call");
    assert!(!r_self.is_error.unwrap_or(false));
    let body_self = extract_text(&r_self).to_string();

    // prompt.body is typed field — silent skip if listed in private_fields,
    // so both should return the same body regardless.
    assert_eq!(
        body_other, body_self,
        "prompt body must be identical for owner and non-owner"
    );
    assert!(
        body_other.contains("You are Alice"),
        "body must contain expected text; got: {body_other}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_with_as_other_strips_view_extracted_value() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // First write (no snapshot yet)
    let w1 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write1 call");
    assert!(!w1.is_error.unwrap_or(false));

    sleep(Duration::from_secs(1)).await;

    // Second write → snapshot of first is written to history/
    let w2 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write2 call");
    assert!(!w2.is_error.unwrap_or(false));

    // history with view=extra.secret, as=other → value should be null (stripped)
    let h = client
        .call_tool(call_params(
            "persona_history",
            json!({ "id": "alice", "view": "extra.secret", "root": root, "as": "other_id" }),
        ))
        .await
        .expect("history call");
    assert!(!h.is_error.unwrap_or(false));
    let h_text = extract_text(&h);
    let h_items: Vec<Value> = serde_json::from_str(h_text)
        .unwrap_or_else(|e| panic!("parse history json: {e}\ngot: {h_text}"));
    assert_eq!(h_items.len(), 1, "expected 1 snapshot; got: {h_text}");
    assert!(
        h_items[0]["value"].is_null(),
        "private field view must be null for non-owner; got: {h_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn validate_does_not_redact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    let w = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write call");
    assert!(!w.is_error.unwrap_or(false));

    // validate with as=other → should still return ok:true (validates full persona)
    let v = client
        .call_tool(call_params(
            "persona_validate",
            json!({ "id": "alice", "root": root, "as": "other_id" }),
        ))
        .await
        .expect("validate call");
    assert!(!v.is_error.unwrap_or(false));
    let v_text = extract_text(&v);
    assert!(
        v_text.contains("\"ok\":true"),
        "validate must return ok:true regardless of as; got: {v_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_self_modifying_private_field_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // Initial write
    let w1 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write1 call");
    assert!(!w1.is_error.unwrap_or(false));

    const UPDATED_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"
private_fields = ["extra.secret"]

[prompt]
body = "You are Alice."

[extra]
secret = "newsecret"
public = "visible"
"#;

    // Owner updates private field → should succeed
    let w2 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": UPDATED_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write2 call");
    assert!(
        !w2.is_error.unwrap_or(false),
        "owner must be able to update private field; got: {}",
        extract_text(&w2)
    );

    // Verify new value is on disk (read as owner)
    let r = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "root": root, "as": "alice" }),
        ))
        .await
        .expect("read call");
    assert!(!r.is_error.unwrap_or(false));
    let r_text = extract_text(&r);
    assert!(
        r_text.contains("newsecret"),
        "updated value must be persisted; got: {r_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_other_modifying_private_value_denied() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // Setup: write initial pack as owner
    let w1 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write1 call");
    assert!(!w1.is_error.unwrap_or(false));

    // Get snapshot count before denied write (after first write, count=0)
    sleep(Duration::from_secs(1)).await;
    let h_before = client
        .call_tool(call_params(
            "persona_history",
            json!({ "id": "alice", "root": root, "as": "alice" }),
        ))
        .await
        .expect("history before call");
    let h_before_text = extract_text(&h_before);
    let h_before_items: Vec<Value> = serde_json::from_str(h_before_text)
        .unwrap_or_else(|e| panic!("parse history: {e}\ngot: {h_before_text}"));
    let snapshot_count_before = h_before_items.len();

    const ATTACK_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"
private_fields = ["extra.secret"]

[prompt]
body = "You are Alice."

[extra]
secret = "stolen"
public = "visible"
"#;

    // Non-owner attempts to modify private value → PermissionDenied
    let w2 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": ATTACK_TOML, "root": root, "as": "other_id" }),
        ))
        .await
        .expect("write2 call");
    assert!(
        w2.is_error.unwrap_or(false),
        "non-owner modifying private value must be denied"
    );
    let err_text = extract_text(&w2);
    assert!(
        err_text.contains("permission denied"),
        "error must say permission denied; got: {err_text}"
    );

    // Zero snapshot: snapshot count must not change
    let h_after = client
        .call_tool(call_params(
            "persona_history",
            json!({ "id": "alice", "root": root, "as": "alice" }),
        ))
        .await
        .expect("history after call");
    let h_after_text = extract_text(&h_after);
    let h_after_items: Vec<Value> = serde_json::from_str(h_after_text)
        .unwrap_or_else(|e| panic!("parse history after: {e}\ngot: {h_after_text}"));
    assert_eq!(
        h_after_items.len(),
        snapshot_count_before,
        "snapshot count must not change on denied write (zero snapshot guarantee)"
    );

    // Zero write: original value must be intact
    let r = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "root": root, "as": "alice" }),
        ))
        .await
        .expect("read call");
    let r_text = extract_text(&r);
    assert!(
        r_text.contains("mysecret"),
        "private value must not have changed; got: {r_text}"
    );
    assert!(
        !r_text.contains("stolen"),
        "attacker's value must not be written; got: {r_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_other_adding_private_fields_denied() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // Write initial pack with no private_fields
    let w1 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PUBLIC_TOML, "root": root }),
        ))
        .await
        .expect("write1 call");
    assert!(!w1.is_error.unwrap_or(false));

    const WITH_PRIVATE: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"
private_fields = ["extra.x"]

[prompt]
body = "You are Alice."

[extra]
x = "hidden"
public = "visible"
"#;

    // Non-owner tries to add private_fields → denied
    let w2 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": WITH_PRIVATE, "root": root, "as": "other_id" }),
        ))
        .await
        .expect("write2 call");
    assert!(
        w2.is_error.unwrap_or(false),
        "adding private_fields by non-owner must be denied"
    );
    let err_text = extract_text(&w2);
    assert!(err_text.contains("permission denied"), "got: {err_text}");

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_other_removing_private_fields_denied() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // Initial write with private_fields
    let w1 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write1 call");
    assert!(!w1.is_error.unwrap_or(false));

    // Non-owner tries to remove private_fields → denied
    let w2 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PUBLIC_TOML, "root": root, "as": "other_id" }),
        ))
        .await
        .expect("write2 call");
    assert!(
        w2.is_error.unwrap_or(false),
        "removing private_fields by non-owner must be denied"
    );
    let err_text = extract_text(&w2);
    assert!(err_text.contains("permission denied"), "got: {err_text}");

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_other_modifying_public_field_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // Initial write with private_fields (secret is private)
    let w1 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write1 call");
    assert!(!w1.is_error.unwrap_or(false));

    const PUBLIC_UPDATE: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"
private_fields = ["extra.secret"]

[prompt]
body = "You are Alice."

[extra]
secret = "mysecret"
public = "updated_value"
"#;

    // Non-owner changes only the public field → allowed
    let w2 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PUBLIC_UPDATE, "root": root, "as": "other_id" }),
        ))
        .await
        .expect("write2 call");
    assert!(
        !w2.is_error.unwrap_or(false),
        "non-owner modifying public field must succeed; got: {}",
        extract_text(&w2)
    );

    // Verify the public field was updated (owner view)
    let r = client
        .call_tool(call_params(
            "persona_read",
            json!({ "id": "alice", "root": root, "as": "alice" }),
        ))
        .await
        .expect("read call");
    let r_text = extract_text(&r);
    assert!(
        r_text.contains("updated_value"),
        "public update must be persisted; got: {r_text}"
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_first_write_with_private_fields_requires_as_self() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // First write with private_fields, as=other → denied
    let w = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "other_id" }),
        ))
        .await
        .expect("write call");
    assert!(
        w.is_error.unwrap_or(false),
        "first write with private_fields by non-owner must be denied"
    );
    let err_text = extract_text(&w);
    assert!(err_text.contains("permission denied"), "got: {err_text}");

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_first_write_no_private_fields_succeeds_anonymous() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // First write with no private_fields, no as → allowed
    let w = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PUBLIC_TOML, "root": root }),
        ))
        .await
        .expect("write call");
    assert!(
        !w.is_error.unwrap_or(false),
        "first write with no private_fields must succeed anonymously; got: {}",
        extract_text(&w)
    );

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_omitted_as_with_private_fields_denied() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap().to_string();

    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    // Setup: write initial pack as owner
    let w1 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": PRIVATE_TOML, "root": root, "as": "alice" }),
        ))
        .await
        .expect("write1 call");
    assert!(!w1.is_error.unwrap_or(false));

    const ATTACK_TOML: &str = r#"
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"
private_fields = ["extra.secret"]

[prompt]
body = "You are Alice."

[extra]
secret = "hacked"
public = "visible"
"#;

    // Anonymous (as omitted) attempts to modify private value → denied
    let w2 = client
        .call_tool(call_params(
            "persona_write",
            json!({ "id": "alice", "toml": ATTACK_TOML, "root": root }),
        ))
        .await
        .expect("write2 call");
    assert!(
        w2.is_error.unwrap_or(false),
        "anonymous write modifying private value must be denied"
    );
    let err_text = extract_text(&w2);
    assert!(err_text.contains("permission denied"), "got: {err_text}");

    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resources_lists_all_guides_and_reads_content() {
    let transport = TokioChildProcess::new(server_command()).expect("spawn server");
    let client = ().serve(transport).await.expect("initialize");

    let resources = client.list_all_resources().await.expect("list resources");
    let uris: Vec<_> = resources.iter().map(|r| r.raw.uri.as_str()).collect();
    for expected in [
        "persona-pack://guides/onboarding",
        "persona-pack://guides/schema",
        "persona-pack://guides/field-private",
        "persona-pack://guides/history",
        "persona-pack://guides/render",
    ] {
        assert!(uris.contains(&expected), "missing resource {expected}");
    }
    for r in &resources {
        assert_eq!(
            r.raw.mime_type.as_deref(),
            Some("text/markdown"),
            "resource {} has wrong mime",
            r.raw.uri
        );
    }

    for uri in [
        "persona-pack://guides/onboarding",
        "persona-pack://guides/schema",
        "persona-pack://guides/field-private",
        "persona-pack://guides/history",
        "persona-pack://guides/render",
    ] {
        let result = client
            .read_resource(ReadResourceRequestParams::new(uri))
            .await
            .expect("read_resource");
        assert!(!result.contents.is_empty(), "no content returned for {uri}");
    }

    let unknown = client
        .read_resource(ReadResourceRequestParams::new(
            "persona-pack://guides/does-not-exist",
        ))
        .await;
    assert!(unknown.is_err(), "expected error for unknown resource URI");

    client.cancel().await.unwrap();
}
