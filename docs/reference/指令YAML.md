# 指令 YAML（Directive，v5）

> **简体中文指南（推荐新手阅读）：** [guide/指令与输入配置.md](../guide/指令与输入配置.md) · [文档中心](../README.md)

指令（directive）是由 `corex-engine` 执行的单个 YAML 文档。

权威来源：[`crates/engine/src/definition.rs`](../../crates/engine/src/definition.rs)，解析器 [`crates/engine/src/resolver.rs`](../../crates/engine/src/resolver.rs)。
编辑器 schema：[`schemas/directive.schema.json`](../../schemas/directive.schema.json)。

## 顶层结构

```yaml
name: hello                 # 必填
description: "..."          # 可选
version: "1.0"              # 可选
inputs: []                  # 可选 InputDecl 列表
variables: {}               # 可选 map → 播种到上下文
triggers: []                # 可选（cron / watch）；手动执行用 corex run
permissions: {}             # 可选 — 省略 = 全部允许（见下文）
steps: []                   # 必填
on_error: abort             # abort | continue | skip（默认 abort）
```

### 输入（Inputs）

```yaml
inputs:
  - name: who
    description: Who to greet
    required: false
    default: "world"
```

已声明的默认值会先经 `{{ }}` 解析，再在调用方未提供该键、或提供了 `null` / 空白字符串时，合并进 `ctx.input`。

### 变量（Variables）

顶层 `variables` 在启动时解析一次，并存入 `ctx.variables`。在步骤上使用 `save_to`，可将该步骤的输出写入变量名。

## 步骤（Steps）

`steps` 是一组无标签（untagged）节点：

| 类型 | 必填键 | 说明 |
|------|--------|------|
| Action | `id`, `action` | 可选 `params`、`save_to`、`when`、`on_error`、`retry` |
| If | `id`, `if`, `then` | 可选 `else` |
| Repeat | `id`, `repeat`, `steps` | `repeat.count` **或** `repeat.each` |
| Parallel | `id`, `parallel` | 可选 `max_concurrency` |

### Action 步骤

```yaml
- id: greet
  action: template.render
  params:
    template: "Hello, {{input.who}}!"
  save_to: message
  when: "{{variables.enabled}}"   # 可选 Condition
  on_error: continue              # 可选覆盖
  retry: 2                        # 可选
```

- **`action`**：Action ID（见 [actions.md](内置Action.md)）。
- **`save_to`**：将步骤结果存入 `variables[<name>]`（亦可直接用 `{{name}}` 引用）。
- 步骤输出始终记录在 `step.<id>` 下，供后续引用。
- **`on_error`**：覆盖顶层策略。`continue` 失败时写入 `Null` 到 `step.<id>` 并继续；`skip` 不写入 `step_outputs`；`abort`（默认）中止流水线。
- **权限拒绝例外**：无论 `on_error` 为何值，`PermissionDenied` **始终中止**该步骤（及整条流水线），避免用 `continue` 绕过门禁。

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
    as: item          # 默认: item
    index: index      # 默认: index（与 each 一起使用）
  steps:
    - id: tick
      action: template.render
      params:
        template: "n={{item}}"
```

或遍历列表：

```yaml
repeat:
  each: "{{items}}"   # 必须解析为列表
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

并发度 = 若设置了 `max_concurrency` 则用其值，否则用配置中的 `runtime.max_parallel`（默认 8）。当有效最大值 **≤ 1**（或仅有一个子步骤）时，步骤 **顺序** 执行。当 **max > 1** 且有多个子步骤时，引擎 **并发** 执行（`buffer_unordered`）。

并行分支若有多个错误，引擎 **优先保留权限拒绝**（`is_permission_denied`），再回退到先遇到的其它失败。

## 条件（`when` / `if`）

无标签形式：

| 形式 | 示例 |
|------|------|
| 表达式字符串 | `"{{variables.enabled}}"`（真值） |
| `eq` / `ne` / `gt` / `lt` | `eq: [a, b]` |
| `and` / `or` / `not` | 嵌套列表 / box |

操作数经同一套 `{{ }}` 解析器解析。

## 占位符解析器

模式（花括号内允许空白）：

| 表达式 | 解析为 |
|--------|--------|
| `{{input.x}}` | 指令输入 `x`（其后可跟可选路径） |
| `{{input}}` | 整个输入 map |
| `{{env.HOME}}` | 环境变量 |
| `{{step.id}}` / `{{steps.id.path}}` | 先前步骤的输出 |
| `{{variables.name}}` / `{{var.name}}` | 变量 |
| `{{name}}` | 裸名：先变量，再输入 |
| `{{directive_input}}` | 可选的整份文档 Directive 输入 Value |

若字符串 **恰好** 是一个 `{{expr}}`，则保留 Value 类型；混合字符串会插值为字符串。

## 权限（Permissions）

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

| 规则 | 行为 |
|------|------|
| **全部标志省略 / false** | **全部允许**（无限制）— 像 `hello.yaml` 这类简单指令无需声明 |
| **任一标志为 `true`** | 仅允许已声明的类别；其余 → permission denied（`on_error: continue` **不能**吞掉） |

类别映射（摘要）：`shell.run` / `exec.run` / bootstrap → shell；`http.send` → network；`clipboard.*` → clipboard；`notify.send` → notifications；`ui.*` → ui；`capture.screenshot` / `capture.monitors` / `capture.ocr` → capture；`keyring.*` → secret；file/copy/scrub/shade/compression/morph/generate.path/codec（除 `codec.json.parse` 外）/capture.crop → filesystem。

在配置中设置 `[runtime].strict_permissions = true`，可 **拒绝** 省略全部权限标志的指令（企业模式）。

## 示例配方（Recipes）

| 目标 | 示例指令 |
|------|----------|
| 索引 / 目录 | [`examples/directives/README.md`](../../examples/directives/README.md) |
| Hello / 输入 | [`hello.yaml`](../../examples/directives/hello.yaml) |
| 占位符解析器 | [`resolver-demo.yaml`](../../examples/directives/resolver-demo.yaml) |
| 控制流 | [`control-flow.yaml`](../../examples/directives/control-flow.yaml), [`control-flow-advanced.yaml`](../../examples/directives/control-flow-advanced.yaml) |
| HTTP GET → 文件 | [`http-save-body.yaml`](../../examples/directives/http-save-body.yaml) |
| HTTP POST JSON | [`http-post-json.yaml`](../../examples/directives/http-post-json.yaml) |
| HTTP → JSON → patch | [`http-extract-patch.yaml`](../../examples/directives/http-extract-patch.yaml) |
| Codec 流水线 | [`codec-pipeline.yaml`](../../examples/directives/codec-pipeline.yaml) |
| file.write 模式 | [`file-write-modes.yaml`](../../examples/directives/file-write-modes.yaml) |
| 时间戳 → JSON/JS | [`inject-build-time.yaml`](../../examples/directives/inject-build-time.yaml) |
| Bing 建议 + 解析 | [`bing-suggest.yaml`](../../examples/directives/bing-suggest.yaml) |
| 剪贴板 + 通知 | [`clipboard-notify.yaml`](../../examples/directives/clipboard-notify.yaml) |
| shell.run 宿主 | [`shell-host-demo.yaml`](../../examples/directives/shell-host-demo.yaml) |
| UI 自动化（脆弱） | [`wechat-send-message.yaml`](../../examples/directives/wechat-send-message.yaml) |

典型链路：`http.send` → `codec.json.parse` → 在 `file.write` 中使用 `{{parsed.field}}`（无需单独的 query action）。

## Triggers

```yaml
triggers:
  - type: cron
    expr: "0 9 * * 1-5"        # 5 或 6 字段；规则见 cron表达式.md
    timezone: local            # 可选：local | utc | +08:00；默认 runtime.cron_timezone=local
  - type: watch
    paths: ["./src"]
    includes: []
    excludes: ["**/node_modules/**"]
    debounce_ms: 300
    throttle_ms: 1200
    immediate: false          # 启动后立即跑一次 pipeline
    poll: false               # NFS/WSL 等不可靠 FS 时用 PollWatcher
    events: []                # 空 = create+modify+remove；可收紧为 ["create","modify"]
```

| cron 字段 | 默认 | 说明 |
|-----------|------|------|
| `expr` | （必填） | 5 或 6 字段 cron 表达式 |
| `timezone` | `runtime.cron_timezone`（默认 `local`） | `local` / `utc` / `±HH:MM` |

| watch 字段 | 默认 | 说明 |
|------------|------|------|
| `paths` | （必填） | 监听根路径（文件或目录） |
| `includes` / `excludes` | `[]` / 内置 `.git`、`node_modules`、`test-results` | glob 过滤（与 copy.run 语义一致） |
| `debounce_ms` | `300` | **FS debounce**：`notify_debouncer_full` 安静期后再发触发信号（不是 lodash debounce） |
| `throttle_ms` | `max(debounce×2, 1000)` | **Throttle 间隔**（必须 `> 0`）。lodash 默认边沿：窗口外首次立即 run（leading），窗口内多次最多再 trailing 一次；窗口从 invoke **开始**计时。旧字段 `cooldown_ms` 会解析失败，请改用本字段 |
| `immediate` | `false` | supervisor 启动后立刻执行一次，并刷新 throttle `last_invoke`；`immediate` 时 register 后至 `run_now` 完成前忽略 FS 事件，避免启动双跑 |
| `poll` | `false` | 使用 PollWatcher 代替 OS 原生 watcher |
| `events` | `[]`（全部内容变更 kind） | 可选白名单：`create`、`modify`、`remove`、`access` |

流水线：`FS 事件 → debounce(debounce_ms) → 触发信号 → throttle(throttle_ms) → run_directive`。详见 [架构 · Watch 事件管道](./架构.md#watch-事件管道)。

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

## 示例（Examples）

多步流水线：

- [`examples/directives/hello.yaml`](../../examples/directives/hello.yaml)
- [`examples/directives/control-flow.yaml`](../../examples/directives/control-flow.yaml)
- [`examples/directives/copy-demo.yaml`](../../examples/directives/copy-demo.yaml)
- [`examples/directives/triggers-declared.yaml`](../../examples/directives/triggers-declared.yaml) — `triggers.cron` / `triggers.watch`
- 索引：[examples/directives/README.md](../../examples/directives/README.md)

单 Action 最小示例：[examples/actions/](../../examples/actions/README.md)

## Related

- [actions.md](内置Action.md)
- [schemas/directive.schema.json](../../schemas/directive.schema.json)
- [ipc-protocol.md](IPC协议.md)
- [architecture.md](架构.md)（含 [Supervisor](架构.md#supervisor-子系统cron--watch)）
- [cron表达式.md](cron表达式.md)（`tokio-cron-scheduler` → croner 3 表达式规则）
