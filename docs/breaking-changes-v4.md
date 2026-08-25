# Breaking Changes — Corex v4.0.0

本版本为**破坏性**重构：workspace、二进制、YAML 与 IPC 均与 v3 / 旧 monolith 不兼容。请整包升级，不要混用旧 `corex-serve` 与新引擎。

## 总览

| 项 | 旧（≤3.x） | 新（4.0.0） |
|----|------------|-------------|
| Edition / version | 多为 2021 + 3.x | **edition 2021**，workspace **`4.0.0`** |
| Daemon 二进制 | `corex-serve` | **`corex-daemon`** |
| 库布局 | 根目录 `corex-core`（lib `cx`）等 | `crates/{core,engine,registry,ipc,plugin-sdk}` + `bins/{cli,daemon}` |
| IPC 传输 | Windows Named Pipe（`--pipe`） | Unix domain socket（`--socket`，默认 `corex.sock`） |
| 编排 DSL | Pipeline v3：`module` + `action` + `${var.*}` | **Shortcut**：`action` id + **`{{ }}`** 占位符 |
| Action 标识 | module 名 / 嵌套 action | 点分 **Action ID**（如 `file.write`） |

## `corex-serve` → `corex-daemon`

```bash
# 旧
cargo run -p corex-serve
corex-serve --pipe \\.\pipe\corex

# 新
cargo run -p corex-daemon
corex-daemon --socket /path/to/corex.sock
corex daemon run   # CLI 拉起同二进制
```

- Release ZIP / Tauri sidecar 逻辑名改为 **`corex-daemon`**（不再打包 `corex-serve`）。
- 帮助校验字段：`--socket`（不再要求 `--pipe`）。
- 协议形状见 `corex-ipc`：`type: ping|shutdown|list_actions|list_shortcuts|run_shortcut|invoke`（与旧 `module`/`action` invoke 不同）。

## YAML：Pipeline v3 → Shortcut

旧（`pipelines.yaml` / Pipeline v3）：

```yaml
version: 3
pipelines:
  - id: build
    steps:
      - id: copy_cache
        module: copy
        params:
          from: '${var.base}/src'
          to: '${var.base}/dist'
```

新（Shortcut，见 `examples/shortcuts/`）：

```yaml
name: hello
inputs:
  - name: who
    default: "world"
variables:
  greeting_prefix: "Hello"
steps:
  - id: greet
    action: template.render
    params:
      template: "{{ greeting_prefix }}, {{ who }}!"
      context:
        greeting_prefix: "{{greeting_prefix}}"
        who: "{{input.who}}"
    save_to: message
```

要点：

- 顶层是单个 **`name`** 快捷指令，不再是 `version: 3` + `pipelines[]`。
- 步骤使用 **`action: <id>`**，不再使用 `module` / `format` / `algorithm` 线格式。
- 占位符由 **`${var.*}` / `${steps.*}`** 改为 **`{{var}}` / `{{input.x}}`** 等（engine resolver）。
- 旧根目录 `pipelines.yaml` 已迁为示例说明；请使用 `examples/shortcuts/*.yaml`。

## Action IDs

内置示例（feature gate）：

| Action ID | Feature |
|-----------|---------|
| `shell.run` | `act-shell` |
| `http.request` | `act-http` |
| `clipboard.get` / `clipboard.set` | `act-clipboard` |
| `notify.send` | `act-notify` |
| `file.read` / `file.write` / … | `act-file` |
| `template.render` | `act-template` |
| `cron.schedule` | `act-cron` |
| `keyring.get` / `keyring.set` | `act-keyring` |

旧业务 module（copy / morph / capture…）将在 P4 以 Action 形式迁入；迁移前请勿假定旧 CLI 子命令仍存在于新 `corex` 二进制。

## CLI 表面

新 CLI 面向 Shortcut，不再暴露旧的 `corex copy` / `corex pipeline` 等子命令树：

```text
corex run | list | actions | create | validate | daemon
```

## 配置

默认配置见 `config/default.toml`（`[daemon]` / `[plugins]` / `[history]` / `[logging]` / `[runtime]`）。运行时 `plugins.disabled` 与 `plugins.disabled_actions` 可禁用插件或单个 Action ID。

## 兼容策略

- **不**提供 Pipeline v3 / Named Pipe / `corex-serve` 双读。
- i-thinking / Tauri 需同步：sidecar 名、socket、协议与资产 ZIP 内容。
- 历史 IPC / capture 破坏性说明仍见 [breaking-changes.md](./breaking-changes.md)（v0.3–v3）；**升级到 v4 时以本文为准**。
