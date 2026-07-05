# History

Bundled with the MCP server at `persona-pack://guides/history`.

Every `persona_write` snapshots the previous `prompt.toml` before overwriting
it, so past versions remain readable via `persona_history` (listing) and
`persona_read` with the `at` parameter (fetching).

## How snapshots are created

On each `persona_write`:

1. If `prompt.toml` already exists, its current content is copied to
   `<root>/<id>/history/<UTC>.toml`.
2. Only after the snapshot copy succeeds is `prompt.toml` overwritten.
3. If the copy fails, the write is aborted — history integrity is never
   sacrificed for a successful overwrite.

The first-ever write for a Pack has nothing to snapshot and is skipped.

## Snapshot filename format

Snapshots are named with a UTC timestamp in the format
`YYYY-MM-DDTHH-MM-SSZ`. Colons from ISO 8601 are replaced with hyphens for
filesystem portability:

```
history/2026-05-06T10-35-12Z.toml
history/2026-05-06T11-02-44Z.toml
```

## Listing history

```json
{ "tool": "persona_history",
  "arguments": { "id": "alice", "view": "extra.version" } }
```

Returns a JSON array sorted by timestamp descending (newest first):

```json
[
  { "timestamp": "2026-05-06T11-02-44Z", "value": "2.0" },
  { "timestamp": "2026-05-06T10-35-12Z", "value": "1.0" }
]
```

Entries where the requested path is absent are omitted from the result.

## The `view` selector

`view` is a dotted path resolved uniformly across every top-level TOML
section — `extra.*`, `meta.*`, `prompt.*`, and any future key. No section is
special-cased; the lookup walks the parsed TOML tree recursively.

| `view`          | Returns per snapshot                     |
|-----------------|------------------------------------------|
| `extra.version` | `[extra] version` value                  |
| `meta.name`     | `[meta] name` value                      |
| `prompt.body`   | `[prompt] body` value                    |

Default when `view` is omitted: `extra.version`.

## Reading a past version

Pass a `timestamp` from `persona_history` as the `at` parameter of
`persona_read`:

```json
{ "tool": "persona_read",
  "arguments": { "id": "alice", "at": "2026-05-06T10-35-12Z" } }
```

The return shape is identical to a normal `persona_read`, but the source is
`history/<at>.toml` instead of the live `prompt.toml`. The current
`prompt.toml` is **not** included in `persona_history` results — use
`persona_read` without `at` to read the live version.

## Field-level privacy interaction

`persona_history` and `persona_read` both accept the `as` parameter. When
`as != meta.id` (or is omitted), private paths declared in
`meta.private_fields` are stripped from snapshot reads too, using the
`private_fields` value from **that snapshot** (each snapshot carries its own
privacy declaration).

See `persona-pack://guides/field-private` for details.
