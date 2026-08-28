# Directive YAML (v5)

> **简体中文指南（推荐新手阅读）：** [guide/指令与输入配置.md](./guide/指令与输入配置.md) · [文档中心](./README.md)

directives are single YAML documents executed by `corex-engine`.

Source of truth: [`crates/engine/src/definition.rs`](../crates/engine/src/definition.rs), resolver [`crates/engine/src/resolver.rs`](../crates/engine/src/resolver.rs).

## Top-level shape

```yaml
name: hello                 # required
description: "..."          # optional
version: "1.0"              # optional
inputs: []                  # optional InputDecl list
variables: {}               # optional map → seeded into context
triggers: []                # optional (cron / watch); manual run via corex run
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

Declared defaults are resolved (`{{ }}`) then merged into `ctx.input` when the caller did not supply the key, or supplied `null` / blank string.

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
| `{{input.x}}` | Directive input `x` (optional path after) |
| `{{input}}` | Entire input map |
| `{{env.HOME}}` | Environment variable |
| `{{step.id}}` / `{{steps.id.path}}` | Prior step output |
| `{{variables.name}}` / `{{var.name}}` | Variable |
| `{{name}}` | Bare: variables first, then input |
| `{{directive_input}}` | Optional whole-document Directive input Value |

A string that is **exactly** one `{{expr}}` keeps the Value type; mixed strings interpolate to a string.

## Permissions

```yaml
permissions:
  network: true
  filesystem: true
  shell: true
  clipboard: true
  notifications: true
  ui: true
  capture: true
  secret: true
```

| Rule | Behavior |
|------|----------|
| **All flags omitted / false** | **Allow-all** (unrestricted) — simple directives like `hello.yaml` need no declarations |
| **Any flag `true`** | Only declared categories are allowed; others → permission denied |

Category mapping (summary): `shell.run` / `exec.run` / bootstrap → shell; `http.send` → network; `clipboard.*` → clipboard; `notify.send` → notifications; `ui.*` → ui; `capture.screenshot` / `capture.monitors` / `capture.ocr` → capture; `keyring.*` → secret; file/copy/scrub/shade/compression/morph/generate.path/codec (except `codec.json.parse`)/capture.crop → filesystem.

Set `[runtime].strict_permissions = true` in config to **deny** directives that omit all permission flags (enterprise mode).

## Recipes

| Goal | Example Directive |
|------|------------------|
| Index / catalog | [`examples/directives/README.md`](../examples/directives/README.md) |
| Hello / inputs | [`hello.yaml`](../examples/directives/hello.yaml) |
| Placeholder resolver | [`resolver-demo.yaml`](../examples/directives/resolver-demo.yaml) |
| Control flow | [`control-flow.yaml`](../examples/directives/control-flow.yaml), [`control-flow-advanced.yaml`](../examples/directives/control-flow-advanced.yaml) |
| HTTP GET → file | [`http-save-body.yaml`](../examples/directives/http-save-body.yaml) |
| HTTP POST JSON | [`http-post-json.yaml`](../examples/directives/http-post-json.yaml) |
| HTTP → JSON → patch | [`http-extract-patch.yaml`](../examples/directives/http-extract-patch.yaml) |
| Codec pipeline | [`codec-pipeline.yaml`](../examples/directives/codec-pipeline.yaml) |
| file.write modes | [`file-write-modes.yaml`](../examples/directives/file-write-modes.yaml) |
| Timestamp → JSON/JS | [`inject-build-time.yaml`](../examples/directives/inject-build-time.yaml) |
| Bing suggest + parse | [`bing-suggest.yaml`](../examples/directives/bing-suggest.yaml) |
| Clipboard + notify | [`clipboard-notify.yaml`](../examples/directives/clipboard-notify.yaml) |
| shell.run host | [`shell-host-demo.yaml`](../examples/directives/shell-host-demo.yaml) |
| UI automation (fragile) | [`wechat-send-message.yaml`](../examples/directives/wechat-send-message.yaml) |

Typical chain: `http.send` → `codec.json.parse` → use `{{parsed.field}}` in `file.write` (no separate query action).

## Triggers

```yaml
triggers:
  - type: cron
    expr: "0 * * * *"          # 5 或 6 字段；启动: corex cron run <name>
  - type: watch
    paths: ["./src"]
    includes: []
    excludes: ["**/node_modules/**"]
    debounce_ms: 300
    cooldown_ms: 1200
    immediate: false          # 启动后立即跑一次 pipeline
    poll: false               # NFS/WSL 等不可靠 FS 时用 PollWatcher
    events: []                # 空 = create+modify+remove；可收紧为 ["create","modify"]
```

| watch 字段 | 默认 | 说明 |
|------------|------|------|
| `paths` | （必填） | 监听根路径（文件或目录） |
| `includes` / `excludes` | `[]` / 内置 `.git`、`node_modules`、`test-results` | glob 过滤（与 copy.run 语义一致） |
| `debounce_ms` / `cooldown_ms` | `300` / `max(debounce×2, 1000)` | 防抖与执行后冷却 |
| `immediate` | `false` | 等价 v3 `--immediate`；supervisor 启动后立刻执行一次 |
| `poll` | `false` | 使用 PollWatcher 代替 OS 原生 watcher |
| `events` | `[]`（全部内容变更 kind） | 可选白名单：`create`、`modify`、`remove`、`access` |

YAML `triggers` 规则：

- 同一指令 **可同时** 声明 **1 个** `watch` **和** **1 个** `cron`（互不排斥）
- 同一指令 **不可** 声明多个 `watch` 或多个 `cron`
- 运行时：每种类型 **最多一个** 守护进程（`corex watch run` 与 `corex cron run` 可同时对同一指令各启一个）
- `paths` / `expr` 支持 `{{variables.*}}`、`{{env.*}}` 等占位符（supervisor 启动时解析，与 steps 相同）

- **`corex run <name>`** — 手动执行（无需在 triggers 声明）
- **`corex schedule`** — 列出可用指令
- **`corex watch run|ps|attach|logs|send|stop|restart`** — 文件监听守护（**操作用指令名，非 pid**）
- **`corex cron run|ps|attach|logs|send|stop|restart`** — cron 守护（同上）

常用 watch/cron 流程：

```text
corex watch run build-client         # 后台启动
corex watch run build-client --immediate   # 启动后立即跑一次
corex watch run build-client --foreground   # 前台开发（Ctrl+C 停止）
corex watch attach build-client      # 查看日志；Ctrl+C 退出查看，不停止守护
corex watch ps                    # NAME 列为指令名
corex watch send build-client run-now
corex watch stop build-client
corex watch stop build-client --force   # 立即终止进行中的构建
corex watch send build-client run-now
```

## Examples

- [`examples/directives/hello.yaml`](../examples/directives/hello.yaml)
- [`examples/directives/control-flow.yaml`](../examples/directives/control-flow.yaml)
- [`examples/directives/copy-demo.yaml`](../examples/directives/copy-demo.yaml)

## Related

- [actions.md](./actions.md)
- [ipc-protocol.md](./ipc-protocol.md)
- [architecture.md](./architecture.md)
