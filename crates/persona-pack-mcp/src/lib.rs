//! MCP service for Persona Pack — 4 dedicated tools over a `personas/` root.

use std::path::PathBuf;

use anyhow::Result;
use persona_pack::{lookup_dot_path, PackRoot, Persona, PersonaError};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use serde::Deserialize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
    /// Optional caller id. Required when the TOML modifies `meta.private_fields`
    /// or any private-path value. Must exactly match `meta.id`; otherwise
    /// the write is rejected with PermissionDenied (zero write, zero snapshot).
    #[serde(rename = "as")]
    pub caller_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReadParams {
    pub id: String,
    pub root: Option<String>,
    /// Optional timestamp to read a historical snapshot from `history/<at>.toml`.
    /// When omitted, the current `prompt.toml` is returned (default behaviour).
    pub at: Option<String>,
    /// Optional caller id. When equal to `meta.id`, the full Persona is returned.
    /// Otherwise `meta.private_fields` path keys are stripped (key-level, not placeholder).
    #[serde(rename = "as")]
    pub caller_id: Option<String>,
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
    /// Optional caller id. When equal to `meta.id`, the full snapshot value is returned.
    /// Otherwise `meta.private_fields` path keys are stripped before view extraction.
    #[serde(rename = "as")]
    pub caller_id: Option<String>,
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
    /// Optional caller id. Accepted for API consistency but not used — schema
    /// validation is always performed against the full, unredacted Persona.
    #[serde(rename = "as")]
    pub caller_id: Option<String>,
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
    /// Optional caller id. When equal to `meta.id`, the full Persona is rendered.
    /// Otherwise `meta.private_fields` path keys are stripped before rendering.
    #[serde(rename = "as")]
    pub caller_id: Option<String>,
}

/// Check whether `caller_id` is permitted to perform a write that transitions
/// from `existing` to `new_persona`.
///
/// Permission is required (caller must match `meta.id`) when:
/// (a) `meta.private_fields` differs between existing and new, OR
/// (b) the value at any path listed in either `private_fields` set changes.
///
/// Returns `Ok(())` if allowed, `Err(reason)` if denied.
fn check_write_permission(
    new_persona: &Persona,
    existing: Option<&Persona>,
    caller_id: Option<&str>,
) -> Result<(), String> {
    let old_private: Vec<String> = existing
        .map(|ep| ep.meta.private_fields.clone())
        .unwrap_or_default();
    let new_private: Vec<String> = new_persona.meta.private_fields.clone();

    // (a) Check if private_fields schema differs.
    let schema_diff = old_private != new_private;

    // (b) Check if any private path value changed.
    // union of old and new private_fields to catch both additions and removals.
    let mut check_paths: Vec<&str> = Vec::new();
    for p in &old_private {
        check_paths.push(p.as_str());
    }
    for p in &new_private {
        if !check_paths.contains(&p.as_str()) {
            check_paths.push(p.as_str());
        }
    }

    // Serialize once outside the per-path loop; log if serialization fails (§1-2-6).
    let existing_tv: Option<toml::Value> = existing.and_then(|ep| {
        match toml::Value::try_from(ep) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(error = %e, "check_write_permission: failed to serialize existing persona to toml::Value");
                None
            }
        }
    });
    let new_tv: Option<toml::Value> = match toml::Value::try_from(new_persona) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "check_write_permission: failed to serialize new persona to toml::Value");
            None
        }
    };

    let value_diff = check_paths.iter().any(|path| {
        let old_val = existing_tv
            .as_ref()
            .and_then(|tv| lookup_dot_path(tv, path).cloned());
        let new_val = new_tv
            .as_ref()
            .and_then(|tv| lookup_dot_path(tv, path).cloned());
        old_val != new_val
    });

    if schema_diff || value_diff {
        let caller_matches = caller_id
            .map(|cid| cid == new_persona.meta.id.as_str())
            .unwrap_or(false);
        if !caller_matches {
            let reason = if schema_diff {
                "private_fields definition changed; as==meta.id required".to_string()
            } else {
                "private field value changed; as==meta.id required".to_string()
            };
            return Err(reason);
        }
    }
    Ok(())
}

/// Extract a value from a `serde_json::Value` using a dot-separated path.
/// Returns `None` if any segment is not found.
fn json_dot_path(value: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current.clone())
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
    ///
    /// # Arguments
    /// * `as` — Optional caller id. Required when the TOML changes
    ///          `meta.private_fields` or any private-path value; must exactly
    ///          match `meta.id`. Violation returns PermissionDenied with zero
    ///          write and zero snapshot.
    #[tool(name = "persona_write", annotations(open_world_hint = false))]
    async fn write(&self, Parameters(p): Parameters<WriteParams>) -> Result<String, String> {
        let new_persona = Persona::from_toml_str(&p.toml).map_err(|e| e.to_string())?;
        if new_persona.meta.id != p.id {
            return Err(format!(
                "id mismatch: path id `{}` != toml meta.id `{}`",
                p.id, new_persona.meta.id
            ));
        }
        let root = self.pack_root(p.root);

        // crux #2: permission MUST be checked before snapshot/write (zero write on deny).
        // Fetch the existing persona (None if this is a first write).
        let existing_persona: Option<Persona> = match root.read(&p.id) {
            Ok(ep) => Some(ep),
            Err(PersonaError::NotFound(_)) => None,
            Err(e) => return Err(e.to_string()),
        };
        if let Err(reason) = check_write_permission(
            &new_persona,
            existing_persona.as_ref(),
            p.caller_id.as_deref(),
        ) {
            tracing::info!(
                id = %p.id,
                caller = ?p.caller_id,
                reason = %reason,
                "persona_write permission denied"
            );
            return Err(format!("permission denied: {reason}"));
        }

        // crux #1: snapshot MUST happen before the overwrite.
        // If snapshot fails, we propagate the error without writing.
        let snapshot_path = root
            .snapshot_before_write(&p.id)
            .map_err(|e| e.to_string())?;
        let path = root.write(&new_persona).map_err(|e| e.to_string())?;
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
    /// * `as`   — Optional caller id. When equal to `meta.id`, full Persona is
    ///            returned. Otherwise `meta.private_fields` path keys are stripped
    ///            (key-level, not placeholder).
    #[tool(name = "persona_read", annotations(open_world_hint = false))]
    async fn read(&self, Parameters(p): Parameters<ReadParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        let persona = match p.at {
            Some(ref ts) => root.read_at(&p.id, ts).map_err(|e| e.to_string())?,
            None => root.read(&p.id).map_err(|e| e.to_string())?,
        };
        let view = persona.redact_for(p.caller_id.as_deref());
        serde_json::to_string(&view).map_err(|e| e.to_string())
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
    ///
    /// # Arguments
    /// * `as` — Optional caller id. When equal to `meta.id`, the full Persona is
    ///          rendered. Otherwise `meta.private_fields` path keys are stripped.
    #[tool(name = "persona_render", annotations(open_world_hint = false))]
    async fn render(&self, Parameters(p): Parameters<RenderParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        let persona = root.read(&p.id).map_err(|e| e.to_string())?;
        let view = persona.redact_for(p.caller_id.as_deref());
        let fmt = p.format.as_deref().unwrap_or("prompt");
        match fmt {
            "prompt" => Ok(view.prompt.body.clone()),
            "header" => {
                let mut out = format!("# {}  (origin: {})\n", view.meta.name, view.meta.origin);
                if let Some(short) = &view.meta.short {
                    out.push_str(short);
                    out.push('\n');
                }
                out.push_str("\n---\n\n");
                out.push_str(&view.prompt.body);
                Ok(out)
            }
            "json" => serde_json::to_string(&view).map_err(|e| e.to_string()),
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
    /// * `as`   — Optional caller id. When equal to `meta.id`, full snapshot
    ///            values are returned. Otherwise `meta.private_fields` path keys
    ///            are stripped before view extraction.
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
                // Deserialize snapshot → redact_for caller → re-serialize to JSON
                // → extract view via dot-path on JSON value.
                let toml_str = match toml::to_string(value) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(timestamp = %ts, error = %e, "history: failed to serialize snapshot toml");
                        return serde_json::json!({ "timestamp": ts, "value": serde_json::Value::Null });
                    }
                };
                let persona = match Persona::from_toml_str(&toml_str) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(timestamp = %ts, error = %e, "history: failed to parse snapshot as Persona");
                        return serde_json::json!({ "timestamp": ts, "value": serde_json::Value::Null });
                    }
                };
                let redacted = persona.redact_for(p.caller_id.as_deref());
                let extracted = serde_json::to_value(&redacted)
                    .ok()
                    .and_then(|jv| json_dot_path(&jv, view))
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
    ///
    /// # Arguments
    /// * `as` — Optional caller id. Accepted for API consistency but not used —
    ///          schema validation is always performed against the full Persona.
    #[tool(name = "persona_validate", annotations(open_world_hint = false))]
    async fn validate(&self, Parameters(p): Parameters<ValidateParams>) -> Result<String, String> {
        let root = self.pack_root(p.root);
        // `as` / caller_id is accepted for API consistency but not used here —
        // schema check always operates on the full, unredacted Persona.
        let _ = &p.caller_id;
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
             - persona_write(id, toml, root?, as?): write `<root>/<id>/prompt.toml`. Validates first.\n\
             - persona_read(id, root?, at?, as?): parse and return the Persona as JSON. `at` selects a history snapshot.\n\
             - persona_render(id, root?, format?, as?): project to a prompt-ready string. format = prompt | header | json.\n\
             - persona_list(root?, origin?): list Pack ids, optionally filtered by origin.\n\
             - persona_validate(id, root?, as?): minimal schema check.\n\
             - persona_info(): show effective root, persona count, version.\n\
             - persona_delete(id, root?): delete <root>/<id>/prompt.toml. Removes the dir if empty after.\n\
             - persona_history(id, view?, root?, as?): list history snapshots (timestamp desc) with view selector. view defaults to \"extra.version\".\n\n\
             Required fields: meta.id, meta.name, prompt.body. Origin: gem|claude|skill|orc|hand|custom:<tag>.\n\
             [extra.*] is preserved untouched.\n\n\
             Field-level privacy: persona_read / render / write / history / validate accept `as: <id>` \
             for field-level private redaction. When `as` equals `meta.id`, the full Persona is returned. \
             Otherwise paths listed in `meta.private_fields` are stripped entirely (key-level, not placeholder). \
             Default (omitted `as`) is anonymous — private fields are stripped."
                .into(),
        );
        info
    }
}

/// Start the `persona-pack-mcp` stdio MCP server.
///
/// This is the full bootstrap sequence previously in `main.rs`, now exported
/// so that `persona-pack-cli`'s `mcp` subcommand is the sole entry point.
/// The Tokio runtime must be provided by the caller.
pub async fn run() -> Result<()> {
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
