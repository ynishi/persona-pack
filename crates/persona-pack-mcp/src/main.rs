//! `persona-pack-mcp` stdio MCP server entry point.

use std::path::PathBuf;

use anyhow::Result;
use persona_pack_mcp::PersonaPackService;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let default_root = std::env::var("PERSONA_PACK_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Default to a stable user-data location so the server never
            // accidentally reads/writes Personas under whatever happens to be
            // CWD at launch time.
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join("persona-pack"))
                .unwrap_or_else(|| PathBuf::from("./persona-pack"))
        });

    if let Err(e) = std::fs::create_dir_all(&default_root) {
        // Don't bail — write tools surface their own error if the root is
        // truly unwritable. Read/list still work for a missing root.
        tracing::warn!(root = %default_root.display(), error = %e, "could not ensure root dir");
    }

    tracing::info!(root = %default_root.display(), "persona-pack-mcp starting");

    let service = PersonaPackService::new(default_root);
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
