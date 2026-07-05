# Field-level privacy

Bundled with the MCP server at `persona-pack://guides/field-private`.

Field-level privacy lets a Persona owner mark parts of their Pack as private,
so that other callers (or anonymous callers) receive a redacted view without
those keys.

## Declaring private paths

Add `meta.private_fields` as a list of TOML dotted paths:

```toml
[meta]
id             = "alice"
private_fields = ["extra.secret", "extra.notes.internal"]

[extra]
secret = "s3cret"
public = "hello"

[extra.notes]
internal = "for alice only"
public   = "for everyone"
```

## Caller identity: the `as` parameter

The `as` parameter is accepted by `persona_read`, `persona_render`,
`persona_history`, `persona_validate`, and `persona_write`. It is also
accepted by the `persona-pack list` / `dump` CLI subcommands as `--as`.

Semantics:

| `as` value            | Behaviour                                              |
|-----------------------|--------------------------------------------------------|
| omitted               | Anonymous — private paths are stripped.                |
| equal to `meta.id`    | Owner — full Persona is returned unchanged.            |
| any other id          | Non-owner — private paths are stripped.                |

## Read example

Alice reading her own Pack:

```json
{ "tool": "persona_read",
  "arguments": { "id": "alice", "as": "alice" } }
```

Response includes `extra.secret` and `extra.notes.internal`.

Bob (or any other caller) reading Alice's Pack:

```json
{ "tool": "persona_read",
  "arguments": { "id": "alice", "as": "bob" } }
```

Response has `extra.secret` **removed entirely** (key-level strip, not a
placeholder value like `"***"`). `extra.public` and `extra.notes.public`
remain.

Anonymous read returns the same redacted shape as Bob's read.

## Write guard

`persona_write` rejects any call that modifies `meta.private_fields` itself,
or the value at any currently-declared private path, unless `as == meta.id`.
On rejection the tool returns `PermissionDenied` with zero write and zero
snapshot — the on-disk state is never changed.

## Scope

Only paths under `[extra.*]` are redacted. Typed schema fields (`meta.*`,
`prompt.*`) are silently skipped when they appear in `private_fields`, so a
redacted Persona still parses as a schema-valid Pack.

This is an **honor-system** design: the server trusts the caller's
self-declared `as` identity. It is a soft partition for cross-persona
composition, not a security boundary against an adversarial caller.

## Related

- `persona-pack://guides/schema` — where `[extra.*]` fits in the shape.
- `persona-pack://guides/history` — `as` applies to snapshot reads too.
