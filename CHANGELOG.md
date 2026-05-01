# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.1.0]: https://github.com/ynishi/persona-pack/releases/tag/v0.1.0
