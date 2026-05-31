use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Args, Parser, Subcommand};
use persona_pack::PackRoot;

#[derive(Parser)]
#[command(name = "persona-pack", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List persona IDs under the root, optionally filtered by origin.
    List(ListArgs),
    /// Dump a persona as JSON.
    Dump(DumpArgs),
    /// Start the MCP server (stdio).
    Mcp,
}

#[derive(Args)]
struct ListArgs {
    /// Filter by origin (e.g. gem, claude, hand, custom:tag).
    #[arg(long)]
    origin: Option<String>,
    /// Root directory. Overrides PERSONA_PACK_ROOT env.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Caller identity. When equal to meta.id, full Persona is returned;
    /// otherwise private fields are stripped. Currently only affects internal
    /// filter; private fields are never returned by list.
    #[arg(long = "as", value_name = "ID")]
    as_id: Option<String>,
}

#[derive(Args)]
struct DumpArgs {
    /// Persona ID to dump.
    id: String,
    /// History snapshot timestamp (e.g. 2024-01-01T00-00-00Z).
    #[arg(long)]
    at: Option<String>,
    /// Root directory. Overrides PERSONA_PACK_ROOT env.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Caller identity. When equal to meta.id, full Persona is returned;
    /// otherwise private fields are stripped.
    #[arg(long = "as", value_name = "ID")]
    as_id: Option<String>,
}

/// Resolve the pack root from CLI arg → PERSONA_PACK_ROOT env → ~/persona-pack → ./persona-pack.
fn resolve_root(arg: Option<PathBuf>) -> PathBuf {
    if let Some(p) = arg {
        return p;
    }
    if let Ok(v) = std::env::var("PERSONA_PACK_ROOT") {
        return PathBuf::from(v);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("persona-pack");
    }
    PathBuf::from("./persona-pack")
}

fn list_cmd(args: ListArgs) -> anyhow::Result<()> {
    let root = PackRoot::new(resolve_root(args.root));
    let ids = root.list().context("failed to list personas")?;
    let filtered: Vec<String> = if let Some(ref origin_filter) = args.origin {
        ids.into_iter()
            .filter(|id| match root.read(id) {
                Ok(p) => {
                    let persona = p.redact_for(args.as_id.as_deref());
                    persona.meta.origin == *origin_filter
                }
                Err(e) => {
                    eprintln!("warn: failed to read persona '{}': {}", id, e);
                    false
                }
            })
            .collect()
    } else {
        ids
    };
    println!("{}", serde_json::to_string(&filtered)?);
    Ok(())
}

fn dump_cmd(args: DumpArgs) -> anyhow::Result<()> {
    let root = PackRoot::new(resolve_root(args.root));
    let persona = match args.at {
        Some(ref ts) => root
            .read_at(&args.id, ts)
            .with_context(|| format!("failed to read persona '{}' at '{}'", args.id, ts))?,
        None => root
            .read(&args.id)
            .with_context(|| format!("failed to read persona '{}'", args.id))?,
    }
    .redact_for(args.as_id.as_deref());
    println!("{}", serde_json::to_string_pretty(&persona)?);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Commands::List(a) => list_cmd(a),
        Commands::Dump(a) => dump_cmd(a),
        Commands::Mcp => persona_pack_mcp::run().await,
    }
}
