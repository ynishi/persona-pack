//! End-to-end test: spawn `persona-pack mcp` as a child process and verify
//! the MCP server responds correctly to `tools/list` via the rmcp client.
//!
//! Verifies Crux 2: `persona-pack mcp` subcommand behaviour matches the
//! standalone server bootstrap (all 8 tools present, stdio transport).

use rmcp::{transport::TokioChildProcess, ServiceExt};
use tokio::process::Command;

/// Build the `persona-pack mcp` command using the cargo-resolved binary path.
fn mcp_command() -> Command {
    // CARGO_BIN_EXE_persona-pack is set by cargo when running tests for a
    // package that declares a [[bin]] named "persona-pack".  Cargo builds the
    // binary automatically before running the test suite.
    let bin = env!("CARGO_BIN_EXE_persona-pack");
    let mut cmd = Command::new(bin);
    cmd.arg("mcp");
    cmd
}

/// Verify that `persona-pack mcp` exposes all 8 expected MCP tools.
///
/// This test satisfies Crux 2 (MCP subcommand behaviour complete-match): the
/// server started via `persona-pack mcp` must expose the same tools as the
/// bootstrap inside `persona_pack_mcp::run()`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_subcommand_lists_all_tools() {
    // Spawn the MCP server child process and initialise an rmcp client.
    // Safety: TokioChildProcess::new panics only on OS-level spawn failure,
    // which would indicate a broken test environment rather than a code bug.
    let transport = TokioChildProcess::new(mcp_command()).expect("spawn persona-pack mcp");
    let client = ().serve(transport).await.expect("rmcp client initialise");

    // list_all_tools() fetches all pages (pagination-safe, preferred over
    // list_tools with a default cursor which may be page-limited).
    let tools = client.list_all_tools().await.expect("list tools");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    for expected in [
        "persona_write",
        "persona_read",
        "persona_list",
        "persona_render",
        "persona_history",
        "persona_validate",
        "persona_delete",
        "persona_info",
    ] {
        assert!(names.contains(&expected), "missing tool: {expected}");
    }

    assert_eq!(
        tools.len(),
        8,
        "expected exactly 8 tools, got {}",
        tools.len()
    );

    // Safety: cancel() sends a shutdown signal; unwrap is acceptable in test
    // context because a cancellation failure does not affect correctness.
    client.cancel().await.unwrap();
}
