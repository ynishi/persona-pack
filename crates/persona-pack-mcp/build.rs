//! Build-time sync guard for the bundled guide docs.
//!
//! The crate ships synced copies of the canonical workspace guides
//! `docs/guides/*.md` at `crates/persona-pack-mcp/guides/*.md` so that
//! `cargo publish` (which packages only files inside the crate's own tree)
//! can include them via `include_str!("../guides/*.md")`.
//!
//! This build script protects the in-sync invariant:
//!
//! - **Dev build (workspace `docs/guides/*.md` exists)** — byte-compare each
//!   pair and `panic!` with a one-line fix command if any diverge.
//! - **Published-tarball build (workspace copy absent)** — skip (only the
//!   in-crate copies ship in the tarball; `include_str!` failure = safety
//!   net #1 is sufficient there).
//!
//! Pattern is intentionally identical to `persona-wire-mcp`'s onboarding
//! guard so the sync SOP is uniform across the workspace.

use std::fs;
use std::path::PathBuf;

const GUIDES: &[&str] = &["onboarding", "schema", "field-private", "history", "render"];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    for name in GUIDES {
        let filename = format!("{name}.md");
        let crate_copy = manifest_dir.join("guides").join(&filename);
        let workspace_copy = manifest_dir.join("../../docs/guides").join(&filename);

        println!("cargo:rerun-if-changed=guides/{filename}");
        println!("cargo:rerun-if-changed=../../docs/guides/{filename}");

        if !workspace_copy.exists() {
            // Published tarball build — only the in-crate copy ships,
            // nothing to compare against. Safety net #1 (include_str!
            // fails if the bundled copy is missing) is sufficient.
            continue;
        }

        let crate_bytes =
            fs::read(&crate_copy).unwrap_or_else(|e| panic!("read {:?}: {e}", crate_copy));
        let workspace_bytes =
            fs::read(&workspace_copy).unwrap_or_else(|e| panic!("read {:?}: {e}", workspace_copy));

        if crate_bytes != workspace_bytes {
            panic!(
                "guide sync drift detected for `{name}`.\n  \
                 canonical: {workspace}\n  \
                 bundled:   {bundled}\n  \
                 fix: cp {workspace} {bundled}",
                workspace = workspace_copy.display(),
                bundled = crate_copy.display()
            );
        }
    }
}
