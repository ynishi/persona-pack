# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.5.0] - 2026-07-05

### Added
- `persona-pack-mcp` now advertises the `resources` capability and exposes five bundled guides at `persona-pack://guides/{onboarding,schema,field-private,history,render}` (`text/markdown`). Clients can discover them via `resources/list` and fetch content via `read_resource`.
- Canonical guides live under `docs/guides/`; the crate ships in-tree copies at `crates/persona-pack-mcp/guides/` and a `build.rs` guard byte-compares the pairs on every dev build to prevent drift. `include_str!` inside the crate directory keeps `cargo publish` self-contained.
- `persona_info` output gains a `resources` field listing the five guide URIs.

## [0.4.0] - 2026-05-31

### Added
- `meta.private_fields` schema field for declarative field-level private (TOML dotted paths, literal only).
- `Persona::redact_for(as_id)` core API returning a redacted clone (key-strip, not placeholder).
- `as` argument on `persona_read` / `persona_render` / `persona_write` / `persona_history` / `persona_validate` MCP tools (honor system).
- `persona_write` permission guard: mutations on private fields or private path values require `as == meta.id` (returns `PermissionDenied` with zero write).
- `--as <ID>` flag on `persona-pack list` / `persona-pack dump` CLI subcommands.
- `PersonaError::PermissionDenied` variant.

## [0.3.0] - 2026-05-07

### Added
- New `persona-pack-cli` crate providing the `persona-pack` binary.
- `persona-pack list [--origin <X>] [--root <path>]` — list Pack ids as JSON array.
- `persona-pack dump <id> [--at <ts>] [--root <path>]` — dump a Persona as JSON (matches `persona_read` MCP tool shape).
- `persona-pack mcp` — start the MCP server over stdio (replaces the standalone `persona-pack-mcp` binary).
- `persona_history(id, view?, root?)` MCP tool: returns snapshot history for a Pack, sorted by timestamp descending (newest first). An optional `view` dot-path selector extracts a specific field from each snapshot across all top-level TOML sections (`extra.*`, `meta.*`, `prompt.*`, and any future key) without hardcoding per-section logic.
- `persona_read` gains an optional `at` parameter: pass a timestamp string from a history entry to read the Pack as it existed at that snapshot.
- `PackRoot::snapshot_before_write(id)` lib helper: copies `prompt.toml` to `history/<UTC>.toml` before every write (skipped on first write; copy failure aborts the write).
- `PackRoot::history_list(id)` lib helper: lists `history/*.toml` entries only; never includes the live `prompt.toml`.
- `PackRoot::read_at(id, at)` lib helper: reads a specific history snapshot by timestamp.
- `lookup_dot_path(value, path)` pub fn in `persona-pack`: resolves a dot-separated path against any `toml::Value` tree uniformly.
- `persona_info` `tools` array now includes `persona_history` (7 → 8 tools).

### Changed
- `persona-pack-mcp` is now a library crate exposing `pub async fn run() -> anyhow::Result<()>`. The MCP server bootstrap is invoked via `persona-pack mcp`.
- `persona_write` now snapshots the existing `prompt.toml` into `history/` **before** overwriting it. The first write is unaffected (no prior file to snapshot). A copy failure aborts the write.

### Removed
- The standalone `persona-pack-mcp` binary. Use `persona-pack mcp` instead.

### Security
- `persona_read(at=...)` guards against path traversal in the `at` parameter (same `[A-Za-z0-9_:T-Z]+` allowlist applied before constructing the history file path).

### Migration

Update `.mcp.json` from:

```json
{"mcpServers": {"persona-pack": {"command": "persona-pack-mcp"}}}
```

to:

```json
{"mcpServers": {"persona-pack": {"command": "persona-pack", "args": ["mcp"]}}}
```

## [0.2.0] - 2026-05-02

### Added
- `persona_delete(id, root?)` MCP tool: removes `<root>/<id>/prompt.toml` (and the dir if empty after). Closes the CRUD-D gap.
- `PackRoot::delete(&str)` helper on the lib side.

### Changed
- `persona_info` `tools` array now includes `persona_delete` (6 → 7 tools).

## [0.1.0] - 2026-05-02

Initial public release.

### Added

- `persona-pack` crate: minimum Persona schema (`meta` / `prompt` / `extra`),
  validator, and `PackRoot` filesystem helpers.
- Optional `meta.spec_version` (`MAJOR.MINOR`) so future readers can gate
  features on the schema revision a Pack was authored against. Missing
  value defaults to `"0.1"`.
- `persona-pack-mcp` crate: MCP server exposing six tools
  (`persona_write` / `persona_read` / `persona_render` / `persona_list` /
  `persona_validate` / `persona_info`) over a single Pack root resolved from
  `PERSONA_PACK_ROOT` or defaulting to `~/persona-pack/`.
- Spec document `docs/persona-pack-spec.md` (v0.1).
- Read-only public sample Packs under `examples/` (Alice, Bob).
- LICENSE-MIT and LICENSE-APACHE.

### Security

- `meta.id` and `custom:<tag>` origin tags are restricted to
  `[A-Za-z0-9_-]+`, preventing path-traversal writes via `persona_write`.

[0.4.0]: https://github.com/ynishi/persona-pack/releases/tag/v0.4.0
[0.3.0]: https://github.com/ynishi/persona-pack/releases/tag/v0.3.0
[0.2.0]: https://github.com/ynishi/persona-pack/releases/tag/v0.2.0
[0.1.0]: https://github.com/ynishi/persona-pack/releases/tag/v0.1.0
