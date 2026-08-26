# Shortcut YAML (v4)

Shortcuts are single YAML documents executed by `corex-engine`.

Source of truth: [`crates/engine/src/definition.rs`](../crates/engine/src/definition.rs), resolver [`crates/engine/src/resolver.rs`](../crates/engine/src/resolver.rs).

## Top-level shape

```yaml
name: hello                 # required
description: "..."          # optional
version: "1.0"              # optional
inputs: []                  # optional InputDecl list
variables: {}               # optional map → seeded into context
triggers: []                # optional (manual / cron / file_watch / hotkey)
permissions: {}             # optional — omit = allow-all (see below)
steps: []                   # required
on_error: abort             # abort | continue | skip (default abort)
```

### Inputs

```yaml
inputs:
  - name: who
    description: Who to greet
    required: false
    default: "world"
```

Declared defaults are resolved (`{{ }}`) then merged into `ctx.input` when the caller did not supply the key.

### Variables

Top-level `variables` are resolved once at start and stored in `ctx.variables`. Use `save_to` on a step to write that step’s output into a variable name.

## Steps

`steps` is a list of untagged nodes:

| Kind | Required keys | Notes |
|------|---------------|--------|
| Action | `id`, `action` | Optional `params`, `save_to`, `when`, `on_error`, `retry` |
| If | `id`, `if`, `then` | Optional `else` |
| Repeat | `id`, `repeat`, `steps` | `repeat.count` **or** `repeat.each` |
| Parallel | `id`, `parallel` | Optional `max_concurrency` |

### Action step

```yaml
- id: greet
  action: template.render
  params:
    template: "Hello, {{input.who}}!"
  save_to: message
  when: "{{variables.enabled}}"   # optional Condition
  on_error: continue              # optional override
  retry: 2                        # optional
```

- **`action`**: Action ID (see [actions.md](./actions.md)).
- **`save_to`**: store the step result in `variables[<name>]` (also available as bare `{{name}}`).
- Step outputs are always recorded under `step.<id>` for later references.

### If

```yaml
- id: branch
  if:
    eq: ["{{input.mode}}", "prod"]
  then:
    - id: a
      action: template.render
      params: { template: "prod" }
  else:
    - id: b
      action: template.render
      params: { template: "dev" }
```

### Repeat

```yaml
- id: loop
  repeat:
    count: 3
    as: item          # default: item
    index: index      # default: index (used with each)
  steps:
    - id: tick
      action: template.render
      params:
        template: "n={{item}}"
```

Or iterate a list:

```yaml
repeat:
  each: "{{items}}"   # must resolve to a list
  as: item
  index: i
```

### Parallel

```yaml
- id: fanout
  max_concurrency: 4
  parallel:
    - id: a
      action: template.render
      params: { template: "A" }
    - id: b
      action: template.render
      params: { template: "B" }
```

Concurrency = `max_concurrency` if set, else `runtime.max_parallel` from config (default 8). When the effective max is **≤ 1** (or only one child), steps run **sequentially**. When **max > 1** and there are multiple children, the engine runs them **concurrently** (`buffer_unordered`).

## Conditions (`when` / `if`)

Untagged forms:

| Form | Example |
|------|---------|
| Expression string | `"{{variables.enabled}}"` (truthy) |
| `eq` / `ne` / `gt` / `lt` | `eq: [a, b]` |
| `and` / `or` / `not` | nested lists / box |

Operands are resolved through the same `{{ }}` resolver.

## Placeholder resolver

Patterns (whitespace inside braces allowed):

| Expression | Resolves to |
|------------|-------------|
| `{{input.x}}` | Shortcut input `x` (optional path after) |
| `{{input}}` | Entire input map |
| `{{env.HOME}}` | Environment variable |
| `{{step.id}}` / `{{steps.id.path}}` | Prior step output |
| `{{variables.name}}` / `{{var.name}}` | Variable |
| `{{name}}` | Bare: variables first, then input |
| `{{shortcut_input}}` | Optional whole-document shortcut input Value |

A string that is **exactly** one `{{expr}}` keeps the Value type; mixed strings interpolate to a string.

## Permissions

```yaml
permissions:
  network: true
  filesystem: true
  shell: true
  clipboard: true
  notifications: true
```

| Rule | Behavior |
|------|----------|
| **All flags omitted / false** | **Allow-all** (unrestricted) — simple shortcuts like `hello.yaml` need no declarations |
| **Any flag `true`** | Only declared categories are allowed; others → permission denied |

Category mapping (summary): `shell.run` / `exec.run` / bootstrap → shell; `http.request` → network; clipboard / `capture.clipboard` → clipboard; `notify.send` → notifications; file/copy/scrub/shade/compression/morph/generate.path/capture screenshot|crop|monitors → filesystem.

## Triggers (declared; scheduling separate)

```yaml
triggers:
  - type: manual
  - type: cron
    expr: "0 * * * *"
  - type: file_watch
    path: ./src
    debounce_ms: 300
  - type: hotkey
    keys: "Ctrl+Shift+S"
```

Builtin `cron.schedule` Action currently **errors** (not implemented). Prefer an external scheduler calling `corex run` until a real scheduler lands.

## Examples

- [`examples/shortcuts/hello.yaml`](../examples/shortcuts/hello.yaml)
- [`examples/shortcuts/control-flow.yaml`](../examples/shortcuts/control-flow.yaml)
- [`examples/shortcuts/copy-demo.yaml`](../examples/shortcuts/copy-demo.yaml)

## Related

- [actions.md](./actions.md)
- [ipc-protocol.md](./ipc-protocol.md)
- [architecture.md](./architecture.md)
