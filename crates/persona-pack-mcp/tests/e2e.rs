//! End-to-end smoke test: spawn the stdio server and exercise all 4 tools.

use std::borrow::Cow;

use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ServiceExt};
use serde_json::{json, Value};
use tokio::process::Command;

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
