# Render — projection formats

Bundled with the MCP server at `persona-pack://guides/render`.

`persona_render` projects a Persona Pack into a prompt-ready string. Three
output formats are available, selected via the `format` parameter.

## Formats

| `format`  | Output                                                     |
|-----------|------------------------------------------------------------|
| `prompt`  | The system-prompt body ready to feed into an LLM call.     |
| `header`  | A short, human-readable header (id, name, origin).         |
| `json`    | The full Persona as JSON (same shape as `persona_read`).   |

`prompt` is the default when `format` is omitted.

## Example calls

Prompt body (default):

```json
{ "tool": "persona_render",
  "arguments": { "id": "alice" } }
```

Header for a listing UI:

```json
{ "tool": "persona_render",
  "arguments": { "id": "alice", "format": "header" } }
```

JSON dump equivalent to `persona_read`:

```json
{ "tool": "persona_render",
  "arguments": { "id": "alice", "format": "json" } }
```

## Field-level privacy interaction

`persona_render` accepts the `as` parameter. When `as != meta.id` (or is
omitted), private paths declared in `meta.private_fields` are stripped
**before** projection, so the rendered output never contains private values.

Note that `format = "prompt"` renders `prompt.body`, which is a typed field
and therefore not eligible for private-path stripping — declaring
`prompt.body` in `private_fields` is silently ignored (see
`persona-pack://guides/field-private` §Scope).

## When to use which

- Feeding an LLM: use `prompt`.
- Building a persona picker / list view: use `header`.
- Cross-tool interop where a downstream consumer wants structured data:
  use `json` (or call `persona_read` directly).
