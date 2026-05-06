# persona-pack

Portable persona schema for LLM applications.

> This project is unrelated to the "Persona" video-game series by Atlus.
> "Persona" here refers to the generic LLM concept of a configured agent
> identity (system-prompt + metadata).

A **Persona Pack** is a single directory containing one Persona, designed to be
moved and adapted across origins (Gemini Gem / Claude Code subagent / Skill /
orcs / hand-written / your own custom systems) without losing origin-specific
context. Because each Pack is self-contained, dropping it into git works
trivially — but Pack-level versioning is delegated to the host VCS for now;
see the spec for details.

## Layout

```
<root>/                  # PERSONA_PACK_ROOT (default: ~/persona-pack/)
  <id>/
    prompt.toml          # required (the Pack body)
    entity_assets/       # optional: persistent assets (icons, knowledge files, memory)
    flow_assets/         # optional: ephemeral (chat logs, run-time data)
  registry.toml          # optional: thin id → path index, alongside <id>/ dirs
```

The root directory itself is the personas folder — `<id>/` directories live
directly under it. `registry.toml` (when present) sits next to them; the
implementation lists Packs by subdirectory, so the registry is never confused
with a Pack.

## Minimum schema

```toml
[meta]
spec_version = "0.1"   # optional, MAJOR.MINOR. Missing = "0.1".
id     = "alice"
name   = "Alice"
origin = "hand"        # gem | claude | skill | orc | hand | custom:<tag>

[prompt]
body = """
You are Alice.
"""

# everything else lives under [extra.<origin>.*] or [extra.custom.*]
```

Required: `meta.id`, `meta.name`, `prompt.body`.
Everything under `[extra.*]` is intentionally not validated — origin-specific
adapters handle that.

## Crates

- [`persona-pack`](crates/persona-pack) — schema + minimum validator (lib)
- [`persona-pack-mcp`](crates/persona-pack-mcp) — MCP server with CRUD + validate

## Setup (MCP server)

The `persona-pack-mcp` server reads/writes Persona Packs under a single root
directory. The root is resolved in this order:

1. `PERSONA_PACK_ROOT` environment variable. Settable from either side:
   - **Shell-wide** (e.g. `export PERSONA_PACK_ROOT="$HOME/persona-pack"` in
     `~/.zshenv` / `~/.zshrc` / `~/.bashrc`). Useful when every project should
     see the same root.
   - **Per project**, via the `env` block in your project's `.mcp.json`:
     ```json
     "persona-pack": {
       "command": "persona-pack-mcp",
       "env": { "PERSONA_PACK_ROOT": "/abs/path/to/personas" }
     }
     ```
     Useful when you want the root to be project-specific or to point at a
     dedicated `personas/` directory checked into the repo.
2. **Default**: `~/persona-pack/` (user-data location; created lazily on first
   write).

Per-call overrides are also supported: every tool accepts an optional
`root` parameter that bypasses the configured default for that call.

> **Note:** `examples/` in this repository is a read-only public sample set
> (Alice / Bob). **Do not point `PERSONA_PACK_ROOT` at it.** Use it as a
> template for new Packs, then store your own Personas under your configured
> root.

## Spec

See [`docs/persona-pack-spec.md`](docs/persona-pack-spec.md)
for the full design rationale and origin-specific `[extra.*]` reservations.

## History

`persona_write` snapshots the existing `prompt.toml` into `<root>/<id>/history/<UTC>.toml`
**before** overwriting it with the new content. The first write is skipped (nothing to snapshot).
If the snapshot copy fails, the write is aborted — history integrity is never sacrificed for
a successful overwrite.

Snapshots are named with a UTC timestamp in the format `YYYY-MM-DDTHH-MM-SSZ`
(colons replaced with hyphens for filesystem portability).

### Listing history

```json
{ "tool": "persona_history", "arguments": { "id": "alice", "view": "extra.version" } }
```

Returns a JSON array sorted by timestamp descending (newest first):

```json
[
  { "timestamp": "2026-05-06T11-02-44Z", "value": "2.0" },
  { "timestamp": "2026-05-06T10-35-12Z", "value": "1.0" }
]
```

The optional `root` parameter selects a non-default Pack root, same as all other tools.

### View Selector

The `view` parameter is a dot-path that is resolved uniformly across all top-level TOML
sections — `extra.*`, `meta.*`, `prompt.*`, and any future key. There is no special-casing
per section; the lookup walks the parsed TOML tree recursively for any root key.

Examples:

| view | returns |
|------|---------|
| `extra.version` | value of `[extra] version` in each snapshot |
| `meta.name` | value of `[meta] name` in each snapshot |
| `prompt.body` | value of `[prompt] body` in each snapshot |

When the path is absent in a snapshot the entry is omitted from the result array.

### Reading a past version

Pass the `timestamp` string from a history entry as the `at` parameter to `persona_read`:

```json
{ "tool": "persona_read", "arguments": { "id": "alice", "at": "2026-05-06T10-35-12Z" } }
```

This returns the full Persona as it existed at that snapshot, identical to a normal
`persona_read` but sourced from `history/<at>.toml` instead of the live `prompt.toml`.
The current `prompt.toml` is never included in `persona_history` results — use
`persona_read` (without `at`) to read the live version.

## License

MIT OR Apache-2.0
