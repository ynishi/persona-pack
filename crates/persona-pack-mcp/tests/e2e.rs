//! End-to-end smoke test: spawn the stdio server and exercise all 4 tools.

use std::borrow::Cow;

use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio::time::{sleep, Duration};

fn server_bin() -> String {
    std::env::var("CARGO_BIN_EXE_persona-pack-mcp").unwrap_or_else(|_| {
        format!(
            "{}/../../target/debug/persona-pack-mcp",
            env!("CARGO_MANIFEST_DIR")
        )
    })
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

    let transport = TokioChildProcess::new(Command::new(server_bin())).expect("spawn server");
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

    let transport = TokioChildProcess::new(Command::new(server_bin())).expect("spawn server");
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

    let transport = TokioChildProcess::new(Command::new(server_bin())).expect("spawn server");
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
