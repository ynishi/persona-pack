# Schema

Bundled with the MCP server at `persona-pack://guides/schema`.

## Required fields

A valid Persona TOML must contain the following three values. Anything else
is optional.

| Path          | Type   | Notes                                       |
|---------------|--------|---------------------------------------------|
| `meta.id`     | string | Must match the containing directory name.   |
| `meta.name`   | string | Human-readable display name.                |
| `prompt.body` | string | The system prompt body.                     |

## Optional meta fields

| Path                   | Type          | Notes                                              |
|------------------------|---------------|----------------------------------------------------|
| `meta.spec_version`    | string        | `MAJOR.MINOR`. Missing is treated as `"0.1"`.      |
| `meta.origin`          | string (enum) | See origin values below.                           |
| `meta.private_fields`  | array<string> | Dotted paths hidden from non-owner callers.        |

## Origin values

`meta.origin` is a small closed enum plus a `custom:<tag>` escape hatch:

| Value            | Meaning                                         |
|------------------|-------------------------------------------------|
| `gem`            | Imported from a Gemini Gem.                     |
| `claude`         | Claude Code subagent-style Pack.                |
| `skill`          | Generic Skill or slash-command Pack.            |
| `orc`            | In-house orchestration Pack.                    |
| `hand`           | Hand-written; no upstream origin.               |
| `custom:<tag>`   | Any user-defined origin. `<tag>` is free-form.  |

## `[extra.*]` — the unchecked namespace

Everything under `[extra.*]` is preserved verbatim and **never** validated by
persona-pack. This is the space where origin-specific adapters store their own
metadata (Gem instructions, Claude subagent front-matter, custom fields for
downstream tooling, etc.).

Convention: put origin-specific keys under `[extra.<origin>.*]` and free-form
custom keys under `[extra.custom.*]`, but this is guidance only — the schema
does not enforce it.

## Full example

```toml
[meta]
spec_version   = "0.1"
id             = "alice"
name           = "Alice"
origin         = "hand"
private_fields = ["extra.secret"]

[prompt]
body = """
You are Alice.
"""

[extra]
version = "1.0"

[extra.hand]
notes = "Written from scratch."

[extra.custom]
tone = "formal"
```

## Validation

`persona_validate` performs the minimum check (required fields present, TOML
parses, `origin` is a recognised value). Deeper checks belong to the
origin-specific adapter, not to persona-pack.
