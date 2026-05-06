//! MCP service for Persona Pack — 4 dedicated tools over a `personas/` root.

use std::path::PathBuf;

use persona_pack::{lookup_dot_path, PackRoot, Persona};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::Deserialize;

#[derive(Clone)]
pub struct PersonaPackService {
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    default_root: PathBuf,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WriteParams {
    /// Persona id. Used as `<root>/<id>/prompt.toml` (must match `meta.id`).
    pub id: String,
    /// Full TOML body of `prompt.toml`.
    pub toml: String,
    /// Optional override for the personas root directory. Defaults to the
    /// server's configured root (CWD or `PERSONA_PACK_ROOT` env var).
    pub root: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadParams {
    pub id: String,
    pub root: Option<String>,
    /// Optional timestamp to read a historical snapshot from `history/<at>.toml`.
    /// When omitted, the current `prompt.toml` is returned (default behaviour).
    pub at: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HistoryParams {
    /// Pack identifier.
    pub id: String,
    /// Dot-path selector for value extraction (e.g. `"extra.version"`, `"meta.name"`).
    /// Defaults to `"extra.version"` when omitted.
    pub view: Option<String>,
    /// Optional override for the personas root directory.
    pub root: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListParams {
    pub root: Option<String>,
    /// Filter by `meta.origin` exact match. e.g. "hand", "skill", "custom:internal".
    pub origin: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateParams {
    pub id: String,
    pub root: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteParams {
    pub id: String,
    pub root: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenderParams {
    pub id: String,
    pub root: Option<String>,
    /// Output format. Defaults to "prompt".
    /// - "prompt": just `prompt.body` as plain text.
    /// - "header": prompt body prefixed with a short `# Name (origin)` header.
    /// - "json": raw Persona JSON (same as `persona_read`).
    pub format: Option<String>,
}

#[tool_router]
impl PersonaPackService {
    pub fn new(default_root: PathBuf) -> Self {
        Self {
            tool_router: Self::tool_router(),
            default_root,
        }
    }

    fn pack_root(&self, override_root: Option<String>) -> PackRoot {
        match override_root {
            Some(p) => PackRoot::new(PathBuf::from(p)),
            None => PackRoot::new(self.default_root.clone()),
        }
    }

    /// Write a Persona Pack to `<root>/<id>/prompt.toml`. Validates before writing.
    /// `id` must match `meta.id` inside the TOML.
    ///
    /// If an existing `prompt.toml` is present it is first copied to
    /// `<root>/<id>/history/<UTC-ts>.toml` (crux #1: copy-before-overwrite).
    /// A copy failure is treated as a write failure (the overwrite is not attempted).
    #[tool(name = "persona_write", annotations(open_world_hint = false))]
    async fn write(&self, Parameters(p): Parameters<WriteParams>) -> Result<String, String> {
        let persona = Persona::from_toml_str(&p.toml).map_err(|e| e.to_string())?;
        if persona.meta.id != p.id {
            return Err(format!(
                "id mismatch: path id `{}` != toml meta.id `{}`",
                p.id, persona.meta.id
            ));
        }
        let root = self.pack_root(p.root);
        // crux #1: snapshot MUST happen before the overwrite.
        // If snapshot fails, we propagate the error without writing.
        let snapshot_path = root
            .snapshot_before_write(&p.id)
            .map_err(|e| e.to_string())?;
        let path = root.write(&persona).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "ok": true,
            "path": path.display().to_string(),
            "snapshot": snapshot_path.map(|p| p.display().to_string()),
        })
        .to_string())
    }

    /// Read and parse a Persona as JSON.
    ///
    /// When `at` is omitted the current `<root>/<id>/prompt.toml` is returned
    /// (backward-compatible default).  When `at` is supplied it selects the
    /// corresponding `<root>/<id>/history/<at>.toml` snapshot.
    ///
    /// # Arguments
    /// * `id`   — Pack identifier
    /// * `root` — Optional root override
    /// * `at`   — Optional UTC timestamp string for a historical snapshot
    #[tool(name = "persona_read", annotations(open_world_hint = false))]
    async fn read(&self, Parameters(p): Parameters<ReadParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        let persona = match p.at {
            Some(ref ts) => root.read_at(&p.id, ts).map_err(|e| e.to_string())?,
            None => root.read(&p.id).map_err(|e| e.to_string())?,
        };
        serde_json::to_string(&persona).map_err(|e| e.to_string())
    }

    /// List all Pack ids under the root. Optionally filter by `meta.origin`.
    #[tool(name = "persona_list", annotations(open_world_hint = false))]
    async fn list(&self, Parameters(p): Parameters<ListParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        let ids = root.list().map_err(|e| e.to_string())?;
        let entries: Vec<_> = ids
            .into_iter()
            .filter_map(|id| {
                let persona = root.read(&id).ok()?;
                if let Some(filter) = &p.origin {
                    if &persona.meta.origin != filter {
                        return None;
                    }
                }
                Some(serde_json::json!({
                    "id": persona.meta.id,
                    "name": persona.meta.name,
                    "origin": persona.meta.origin,
                    "short": persona.meta.short,
                }))
            })
            .collect();
        Ok(serde_json::json!({ "personas": entries }).to_string())
    }

    /// Render a Persona as a prompt-ready string. Use this when feeding the
    /// Persona into an LLM as a system prompt (no JSON wrapping).
    ///
    /// `format` selects the projection:
    /// - "prompt" (default): `prompt.body` as plain text.
    /// - "header": body prefixed with `# Name  (origin: ...)` and `short` (if any).
    /// - "json": raw Persona JSON (same as `persona_read`).
    #[tool(name = "persona_render", annotations(open_world_hint = false))]
    async fn render(&self, Parameters(p): Parameters<RenderParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        let persona = root.read(&p.id).map_err(|e| e.to_string())?;
        let fmt = p.format.as_deref().unwrap_or("prompt");
        match fmt {
            "prompt" => Ok(persona.prompt.body.clone()),
            "header" => {
                let mut out = format!(
                    "# {}  (origin: {})\n",
                    persona.meta.name, persona.meta.origin
                );
                if let Some(short) = &persona.meta.short {
                    out.push_str(short);
                    out.push('\n');
                }
                out.push_str("\n---\n\n");
                out.push_str(&persona.prompt.body);
                Ok(out)
            }
            "json" => serde_json::to_string(&persona).map_err(|e| e.to_string()),
            other => Err(format!(
                "unknown format `{other}` (expected: prompt | header | json)"
            )),
        }
    }

    /// List historical snapshots for `<root>/<id>/history/*.toml` in descending
    /// timestamp order (most recent first).
    ///
    /// Each entry in the returned JSON array has the shape:
    /// `{ "timestamp": "<ts>", "value": <extracted-value-or-null> }`.
    ///
    /// # Arguments
    /// * `id`   — Pack identifier
    /// * `view` — Dot-path selector (e.g. `"extra.version"`, `"meta.name"`).
    ///            Defaults to `"extra.version"` when omitted.
    /// * `root` — Optional root override
    ///
    /// # Constraints (crux #3)
    /// Only `history/*.toml` files are included. The current `prompt.toml` is
    /// never included, even when `view` is not supplied.
    #[tool(
        name = "persona_history",
        annotations(open_world_hint = false, read_only_hint = true)
    )]
    async fn history(&self, Parameters(p): Parameters<HistoryParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        // crux #3: only history_list is called; root.read (prompt.toml) is never added.
        let snapshots = root.history_list(&p.id).map_err(|e| e.to_string())?;
        let view = p.view.as_deref().unwrap_or("extra.version");
        let entries: Vec<_> = snapshots
            .iter()
            .map(|(ts, value)| {
                // crux #2: lookup_dot_path traverses any root key uniformly.
                // No root-specific branching is performed here.
                let extracted = lookup_dot_path(value, view)
                    .and_then(|v| serde_json::to_value(v).ok())
                    .unwrap_or(serde_json::Value::Null);
                serde_json::json!({
                    "timestamp": ts,
                    "value": extracted,
                })
            })
            .collect();
        serde_json::to_string(&entries).map_err(|e| e.to_string())
    }

    /// Validate `<root>/<id>/prompt.toml` against the minimum spec.
    /// Returns `{ "ok": true }` or `{ "ok": false, "error": "..." }`.
    #[tool(name = "persona_validate", annotations(open_world_hint = false))]
    async fn validate(&self, Parameters(p): Parameters<ValidateParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        match root.read(&p.id) {
            Ok(_) => Ok(serde_json::json!({ "ok": true }).to_string()),
            Err(e) => Ok(serde_json::json!({ "ok": false, "error": e.to_string() }).to_string()),
        }
    }

    /// Delete `<root>/<id>/prompt.toml`. Also removes the dir if empty after.
    /// Returns `{ "ok": true, "path": "<deleted path>" }` on success.
    #[tool(name = "persona_delete", annotations(open_world_hint = false))]
    async fn delete(&self, Parameters(p): Parameters<DeleteParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        let path = root.delete(&p.id).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "ok": true, "path": path.display().to_string() }).to_string())
    }

    /// Server diagnostics: which root is in effect, version, and quick stats.
    /// Use this when you need to confirm where Personas are being read/written.
    #[tool(
        name = "persona_info",
        annotations(open_world_hint = false, read_only_hint = true)
    )]
    async fn info(&self) -> Result<String, String> {
        let root = PackRoot::new(self.default_root.clone());
        let exists = self.default_root.exists();
        let count = root.list().map(|v| v.len()).unwrap_or(0);
        Ok(serde_json::json!({
            "root": self.default_root.display().to_string(),
            "root_exists": exists,
            "persona_count": count,
            "version": env!("CARGO_PKG_VERSION"),
            "tools": [
                "persona_write", "persona_read", "persona_render",
                "persona_list", "persona_validate", "persona_info",
                "persona_delete", "persona_history"
            ],
        })
        .to_string())
    }
}

#[tool_handler]
impl ServerHandler for PersonaPackService {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "persona-pack — portable Persona schema (1 dir = 1 Pack).\n\n\
             Tools:\n\
             - persona_write(id, toml, root?): write `<root>/<id>/prompt.toml`. Validates first.\n\
             - persona_read(id, root?, at?): parse and return the Persona as JSON. `at` selects a history snapshot.\n\
             - persona_render(id, root?, format?): project to a prompt-ready string. format = prompt | header | json.\n\
             - persona_list(root?, origin?): list Pack ids, optionally filtered by origin.\n\
             - persona_validate(id, root?): minimal schema check.\n\
             - persona_info(): show effective root, persona count, version.\n\
             - persona_delete(id, root?): delete <root>/<id>/prompt.toml. Removes the dir if empty after.\n\
             - persona_history(id, view?, root?): list history snapshots (timestamp desc) with view selector. view defaults to \"extra.version\".\n\n\
             Required fields: meta.id, meta.name, prompt.body. Origin: gem|claude|skill|orc|hand|custom:<tag>.\n\
             [extra.*] is preserved untouched."
                .into(),
        );
        info
    }
}
