# Corex 架构（v5）

Corex 是可组合的 **指令（Directive）/ Action** 运行时：YAML 定义流水线，内置与 WASM 插件提供动作，CLI 与 Daemon 共用同一套引擎。

## Workspace 布局

```
crates/
  core/          # corex-core — Value / Action / ExecutionContext / Error
  engine/        # corex-engine — Directive YAML、变量解析、Pipeline、历史
  registry/      # corex-registry — 动作注册表、内置 Action、WASM host
  ipc/           # corex-ipc — NDJSON 协议 + Unix socket / Named Pipe
  plugin-sdk/    # corex-plugin-sdk — WIT 契约（corex:plugin-sdk@0.1.0）
bins/
  cli/           # corex — 命令行入口
  daemon/        # corex-daemon — 后台 IPC 服务
pdfium/          # 构建辅助：bundled pdfium（morph 等）
plugins/         # 第三方 *.wasm 插件目录说明
config/
  corex.toml     # 运行时配置
examples/
  directives/     # Directive YAML 示例
  legacy/        # ≤v3 Pipeline 样例（仅历史）
  tauri/         # Tauri sidecar 客户端示例
```

| Crate / Binary | 职责 |
|----------------|------|
| `corex-core` | 核心类型与 `Action` / `ActionStore` trait |
| `corex-engine` | Directive 定义、`{{ }}` 解析、控制流、执行历史 |
| `corex-registry` | 内置动作 +（可选）`WasmPluginHost` 发现/加载 |
| `corex-ipc` | `Request` / `Response`；Unix socket / Windows Named Pipe |
| `corex-plugin-sdk` | WIT world `corex-action` |
| `corex` | CLI：`run` / `list` / `actions` / `create` / `validate` / `repl` / `daemon` |
| `corex-daemon` | 长驻：注册动作、发现插件、执行 Directive、IPC |

## 执行模型

```
Directive YAML
    │
    ▼
corex-engine::Pipeline  ──resolve {{ }}──► ActionStore.get_action(id)
    │                                              │
    │                                              ▼
    │                                    corex-registry builtins
    │                                    + WASM plugins (feature wasm)
    ▼
Value 结果  +  可选 history.jsonl
```

- **Action ID**：点分命名，如 `template.render`、`copy.run`（见 [actions.md](./actions.md)）。
- **占位符**：`{{var}}`、`{{input.x}}`、`{{env.X}}`、`{{step.id}}`（见 [directive-yaml.md](./directive-yaml.md)）。
- **控制流**：`if` / `repeat` / `parallel`。
  - **`parallel`**：当有效并发度（步骤 `max_concurrency` 或配置 `runtime.max_parallel`）**> 1** 且子步骤多于 1 个时，使用 `buffer_unordered` **真正并发**；否则顺序执行。

## 双模式

| 模式 | 入口 | 说明 |
|------|------|------|
| CLI | `corex run <name\|path>` | 进程内加载 registry + Pipeline |
| Daemon | `corex-daemon` / `corex daemon run` | Unix：`<data-dir>/corex.sock`；Windows：`\\.\pipe\corex` |
| REPL | `corex repl` | `help` / `actions` / `list` / `run` / `quit` |

数据目录默认由 `directories` 解析为平台 project data（fallback `.corex/`）。IPC 需 `auth_token`（见 [ipc-protocol.md](./ipc-protocol.md)）。

## WASM 插件

见 [plugins/README.md](../plugins/README.md)。Daemon 启动时扫描 `*.wasm`；bindgen 完全接线前，失败的插件会被 discovery 记录并跳过。

## 配置（已接线）

[`config/corex.toml`](../config/corex.toml) 由 CLI/daemon 加载，下列段**生效**：

| 段 | 用途 |
|----|------|
| `[daemon]` | `socket_path`、`lock_path`、`token`（及 `COREX_TOKEN` / token 文件） |
| `[plugins]` | `plugin_dir`、`disabled`、`disabled_actions` |
| `[history]` | JSONL 执行历史开关与文件名 |
| `[logging]` | 级别 / JSON 日志 |
| `[runtime]` | `max_parallel`、`step_timeout_secs`、`strict_permissions`、`filesystem_roots`、`ui_profile` / `ui_max_selector_chain` / `ui_max_settle_ms` |

企业锁定预设见 [`config/enterprise.toml`](../config/enterprise.toml) 与 [enterprise-deploy.md](./enterprise-deploy.md)。威胁边界见 [threat-model.md](./threat-model.md)。

## 构建

```bash
cargo build -p corex -p corex-daemon --release
cargo test --workspace
```

默认启用 `full`（全部 `act-*`）。最小企业构建见 [enterprise-deploy.md](./enterprise-deploy.md#minimal-enterprise-build)。

发布 ZIP（Windows CI）包含 `corex` + `corex-daemon`（+ `pdfium.dll` 若仍捆绑）。

## 相关文档

| 文档 | 说明 |
|------|------|
| [ipc-protocol.md](./ipc-protocol.md) | NDJSON 协议、token、端点 |
| [directive-yaml.md](./directive-yaml.md) | Directive DSL 与占位符 |
| [actions.md](./actions.md) | 内置 Action ID 表 |
| [enterprise-deploy.md](./enterprise-deploy.md) | 企业部署、preset、最小构建、CLI 信任边界 |
| [threat-model.md](./threat-model.md) | 威胁模型与高风险 Action |
| [compliance.md](./compliance.md) | 合规原则与控制项 |
| [cross-platform-backends.md](./cross-platform-backends.md) | Capture/OCR/UI 跨平台后端规划 |
| [breaking-changes-v4.md](./breaking-changes-v4.md) | v4 破坏性变更 |
| [breaking-changes-v5.md](./breaking-changes-v5.md) | v5 Directive 重命名 |
| [tauri-integration.md](./tauri-integration.md) | Tauri + `corex-daemon` |
| [plugins/README.md](../plugins/README.md) | WASM 插件约定 |
| [archive/](./archive/) | ≤v3 历史文档 |
