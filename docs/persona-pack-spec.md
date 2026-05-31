# Persona Pack — Minimum Spec v0.1

A general-purpose schema for managing Personas across heterogeneous origins:
task-execution agents, AGI-like assistants, support bots, and
panel-discussion customer personas. A **Persona** is a portable unit of
"compressed input material for an LLM." The schema is also adaptable to
character-AI scenarios as a secondary use case.

## 1. Physical Layout

```
<root>/                  # PERSONA_PACK_ROOT (default: ~/persona-pack/)
  <persona-id>/
    prompt.toml          # required. The Pack body.
    entity_assets/       # optional. Persistent assets owned by the Persona.
                         #   e.g. icons, memory snapshots, knowledge files.
    flow_assets/         # optional. Ephemeral, run-instance-bound data.
                         #   e.g. chat logs, run-time data, attachments.
  registry.toml          # optional. Thin id → path index, alongside the
                         # <id>/ directories.
```

- `<root>/` itself is the "personas directory"; the `<id>/` directories live
  directly under it.
- A `<persona-id>/` directory is self-contained. Move the directory and the
  Persona moves with it.
- `flow_assets/` is a candidate for `.gitignore` (operational choice).
- The registry is optional — `<root>/*/prompt.toml` glob works for dynamic
  discovery. `registry.toml` is filtered out automatically because the
  implementation only treats subdirectories as Packs.

**v0.1 policy:** Pack data and metadata share the same root. Keeping
`registry.toml` (and any future small meta file) directly inside `<root>/` is
the intended simplicity.

**Future:** if the number of root-level meta files grows to 3–5, split into
two roots:

- `~/persona-pack/` — user data (visible, hand-edited Personas).
- `~/.persona-pack/` — system metadata (dot dir, normally not hand-touched).

The principle is: **user-facing data goes in a visible directory; system
metadata goes in a dot directory.**

## 2. `prompt.toml` — Required Schema

```toml
[meta]
spec_version   = "0.1"                # optional. MAJOR.MINOR; missing = "0.1".
id             = "kebab-or-snake-case-id"   # required, unique
name           = "Display Name"             # required, human-readable
origin         = "hand"                     # required, see §2.2 for reserved values
short          = "one-line description"     # optional but recommended
private_fields = ["extra.secret"]    # optional. Dotted-path key list to hide
                                     # when caller is not the persona itself.
                                     # Literal paths only (no glob/index).

[prompt]
body   = """
Persona body prompt, inline. Markdown or natural language. This text is
fed to the LLM as-is.
"""
synth  = "synthesizer-name"         # optional. Hook name that synthesizes
                                    # `body` from components. When set,
                                    # `body` may be treated as cached output.

[extra.<origin>.<...>]              # origin-reserved namespace, see §3
[extra.custom.<...>]                # free-form, no convention
```

### 2.1 Required Fields

| key | role |
|---|---|
| `meta.id` | unique identifier; key for registry / cross-references |
| `meta.name` | display name |
| `prompt.body` | LLM input material; non-empty (cached synth output is allowed) |

These are the only fields the SchemaChecker validates. Everything else is
left untouched.

### 2.1.1 Schema Version

`meta.spec_version` is **optional** in v0.1. When present it must match
`MAJOR.MINOR` (e.g. `"0.1"`). Readers must treat a missing value as `"0.1"`
(the schema as defined here). The field exists so that future readers can
gate features on the version a Pack was authored against; once v0.2 ships,
authoring tools should write `spec_version` explicitly.

This field describes the **schema** the Pack was authored against. It does
**not** track changes to the Persona's own content over time; see §7
"Pack-level versioning" for that distinction.

### 2.1.2 Field-Level Private (`meta.private_fields`)

Optional list of TOML dotted paths to mark as private. When a caller provides
`as = "<id>"` to read/render tools and `<id> != meta.id`, the listed paths are
**deleted as keys** (not replaced with placeholder or `null`) from the
response. Anonymous callers (no `as`) receive the same redacted view.

Constraints:

- Literal paths only. Glob (`extra.*.secret`) and array index syntax are not
  supported (v0.1 scope).
- Only `extra.*` paths take effect. Typed fields (`meta.X`, `prompt.X`) are
  silently skipped to keep the redacted Persona schema-valid.
- Honor system: callers self-identify; the server does not authenticate.
- `persona_write` enforces that mutations touching the `private_fields` list or
  any private path require `as == meta.id`; violations return `PermissionDenied`
  with zero write and zero snapshot.
- `persona_validate` ignores the `as` argument and validates against the full
  Persona.
- Existing Packs without `private_fields` are treated as fully public (backward
  compatible).

### 2.2 Reserved Origin Values

| value | source | suggested extra namespace |
|---|---|---|
| `gem` | Google Gemini Gem | `extra.gem.*` |
| `claude` | Claude Code subagent | `extra.claude.*` |
| `skill` | generic Skill prompt (stable shared prompt) | none, or optional `extra.skill.*`. `prompt.body` direct is the default. |
| `orc` | orcs (Multi-Persona Panel, OSS) | `extra.orc.*` |
| `hand` | hand-written / other | `extra.custom.*` |
| `custom:<tag>` | anything else, including private systems | `extra.custom.<tag>.*` |

Non-OSS systems are handled via `custom:<tag>`. Public schema stability
guarantees apply only to reserved origins.

## 3. `[extra.*]` Namespaces (Reserved)

Reserved namespaces are agreed-on regions that projection / adapter (Hub)
code reads from. Anything outside the reservation goes under `extra.custom.*`.

### 3.1 `extra.gem`

```toml
[extra.gem]
gem_id          = "..."   # Google-side Gem ID (only when imported)
knowledge_files = []      # paths relative to entity_assets/
```

### 3.2 `extra.orc` (orcs)

```toml
[extra.orc]
role                = "..."         # short role line
background          = "..."         # background narrative
communication_style = "..."         # tone / voice
backend             = "ClaudeApi"   # backend enum
model_name          = "..."         # optional
default_participant = false
[extra.orc.ui]
icon                = "..."         # path under entity_assets/
base_color          = "#......"
```

### 3.3 `extra.claude` (Claude Code subagent)

```toml
[extra.claude]
description     = "..."
model           = "sonnet"          # haiku | sonnet | opus
tools           = ["Read", "Grep"]
permission_mode = "default"
subagent_type   = "..."             # custom agent name
```

### 3.4 `extra.skill`

Generic Skills typically write directly to `prompt.body`. The optional
fields below are available when needed:

```toml
[extra.skill]
focus    = []   # area tags
triggers = []   # activation keywords (depends on the orchestrator)
```

## 4. SchemaChecker (Minimum)

Validation is two-and-a-half steps:

1. The file parses as TOML.
2. The required fields exist and are non-empty:
   `meta.id`, `meta.name`, `prompt.body`.
3. `meta.origin` is either a reserved value or matches `custom:<tag>`.

`[extra.*]` is **not inspected**. Origin-specific adapters validate their own
namespaces.

## 5. MCP Interface (MVP, four tools)

| tool | responsibility |
|---|---|
| `persona.write(id, toml_str)` | write to `personas/<id>/prompt.toml`; runs validate once |
| `persona.read(id)` | return parsed dict |
| `persona.list(filter?)` | list via registry or glob; filter by `origin` / `tags` |
| `persona.validate(id)` | run §4 checks and return the result |

Projection (origin↔origin conversion) and registry auto-update are out of
scope for the MVP. A future `persona.project(id, to="gem"|"orc"|"claude"|...)`
will cover that.

## 6. Operational Flow (MVP)

1. Create a Persona root (`personas/` in a repo, or `~/persona-pack/` as the
   default user-data location).
2. Add `flow_assets/` to `.gitignore` (optional, operational choice).
3. Wire up `persona-pack-mcp` for the five tools.
4. Dogfood: migrate one Persona by hand using `examples/alice` as a template.
5. Once the workflow holds, bulk-import the rest with a script.
6. Add Hub-side adapters (origin↔origin) incrementally as needed.

> The repository's `examples/` directory is a read-only public sample set,
> not a Persona root. Do not configure `PERSONA_PACK_ROOT` to point at it —
> copy from it instead.

## 7. Design Decisions

- **Why `prompt`?** `Soul` / `Agent` / `Character` bind meaning to a "subject."
  Personas span task execution, customer roles in panels, and AGI-like
  behavior — the only common denominator is "input material for an LLM."
  `prompt` names exactly that compression point. The same shape happens to
  fit character-AI use cases too, but that is a downstream consequence, not
  the design driver.
- **Do not normalize.** Splitting capabilities into a relational schema
  collapses origin-specific context. Origin-bound implicit context (Gem
  knowledge files, orc backend metadata, etc.) is preserved losslessly under
  `[extra.<origin>.*]`. Cross-cutting queries are layered on top later via
  an index.
- **Entity vs flow assets.** Persistent belongings and run-time history have
  different lifetimes and Git policies, so they are split at the directory
  level. Heavy-memory Personas use `entity_assets/memory/`; light imports
  (e.g. straight from a Gem export) leave it empty.
- **Keep the registry thin.** Schema enforcement does not live in the
  registry. The registry only guarantees that a `prompt.toml` exists.

## 7. Pack-level Versioning (out of scope for v0.1)

v0.1 does not provide built-in semantics for "this Persona was edited; here
is the previous revision." The concept matters in business / serious
operations (audit trails, rollback, A/B comparison), but supporting it
properly inside a file-based schema requires the host system to participate
(history store, diff projection, reference resolution).

For now, the recommended workarounds are:

- **Host-side VCS.** Put the Pack root in git; each commit is a Pack-level
  revision. This is sufficient for most teams.
- **`extra.custom.version`.** Authors who want a Pack-internal version
  string can write one under `[extra.custom.*]`. The schema does not read
  it; tooling around the Pack can.

A future minor version may add first-class fields (e.g. `meta.revision`,
`meta.parent`) once the operational pattern is clear.

---

This is a v0.1 spec draft. Feedback welcome.
