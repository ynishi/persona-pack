# Onboarding — persona-pack in 5 minutes

Bundled with the MCP server and exposed at `persona-pack://guides/onboarding`
so that clients can fetch it via `read_resource` without leaving the session.

## What persona-pack is

A **Persona Pack** is a single directory (`1 dir = 1 Pack`) that holds the
prompt and metadata for one LLM Persona (agent identity). Packs are portable
across origin systems — Gemini Gem, Claude Code subagent, generic Skill,
in-house orchestration, or hand-written prompts — without losing
origin-specific context.

## Layout

```
<root>/                  # PERSONA_PACK_ROOT (default: ~/persona-pack/)
  <id>/
    prompt.toml          # required (the Pack body)
    history/             # auto-created; snapshots named <UTC>.toml
    entity_assets/       # optional: persistent assets
    flow_assets/         # optional: ephemeral runtime data
  registry.toml          # optional: thin id → path index
```

## Minimum Persona

`prompt.toml`:

```toml
[meta]
id     = "alice"
name   = "Alice"
origin = "hand"          # gem | claude | skill | orc | hand | custom:<tag>

[prompt]
body = """
You are Alice.
"""
```

Required: `meta.id`, `meta.name`, `prompt.body`. Everything under `[extra.*]`
is preserved verbatim and never validated by persona-pack itself —
origin-specific adapters handle that layer.

## First round-trip via MCP

Write, read, render, list:

```json
{ "tool": "persona_write",
  "arguments": { "id": "alice", "toml": "<toml body>" } }

{ "tool": "persona_read",
  "arguments": { "id": "alice" } }

{ "tool": "persona_render",
  "arguments": { "id": "alice", "format": "prompt" } }

{ "tool": "persona_list", "arguments": {} }
```

## Root resolution

`persona-pack-mcp` resolves the personas root in this order:

1. `PERSONA_PACK_ROOT` environment variable (shell-wide or per-project via
   `.mcp.json` `env` block).
2. Default `~/persona-pack/` (created lazily on first write).

Every tool also accepts an optional `root` parameter that overrides the
configured default for that single call.

## Where to go next

Fetch one of the sibling guides:

- `persona-pack://guides/schema` — required fields, origin enum, `[extra.*]`
- `persona-pack://guides/field-private` — hiding fields across callers
- `persona-pack://guides/history` — snapshots, `view` selector, past reads
- `persona-pack://guides/render` — projection formats
