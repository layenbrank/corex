# Corex 架构（v4）

Corex 是可组合的 **快捷指令（Shortcut）/ Action** 运行时：YAML 定义流水线，内置与 WASM 插件提供动作，CLI 与 Daemon 共用同一套引擎。

## Workspace 布局

```
crates/
  core/          # corex-core — Value / Action / ExecutionContext / Error
  engine/        # corex-engine — Shortcut YAML、变量解析、Pipeline、历史
  registry/      # corex-registry — 动作注册表、内置 Action、WASM host
  ipc/           # corex-ipc — Unix socket 传输与请求协议
  plugin-sdk/    # corex-plugin-sdk — WIT 契约（corex:plugin-sdk@0.1.0）
bins/
  cli/           # corex — 命令行入口
  daemon/        # corex-daemon — 后台 IPC 服务
pdfium/          # 构建辅助：bundled pdfium.dll（morph 等仍可能需要）
plugins/         # 第三方 *.wasm 插件目录说明
config/
  default.toml   # 默认运行时配置
examples/
  shortcuts/     # Shortcut YAML 示例
```

| Crate / Binary | 职责 |
|----------------|------|
| `corex-core` | 核心类型与 `Action` / `ActionStore` trait |
| `corex-engine` | Shortcut 定义、`{{var}}` 解析、控制流、执行历史 |
| `corex-registry` | 内置动作 +（可选）`WasmPluginHost` 发现/加载 |
| `corex-ipc` | Daemon 协议（`Request` / `Response`）与 Unix socket |
| `corex-plugin-sdk` | WIT world `corex-action`（`meta` / `validate` / `execute`） |
| `corex` | CLI：`run` / `list` / `actions` / `create` / `validate` / `daemon` |
| `corex-daemon` | 长驻进程：注册动作、发现插件、执行 Shortcut、IPC |

旧 monolith（`corex/`、`corex-core/` 根目录包、`corex-serve/`、`corex-capture/`）已移出 workspace；业务模块迁入 Action 见迁移计划（P4）。

## 执行模型

```
Shortcut YAML
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

- **Action ID**：点分命名，如 `template.render`、`file.write`、`shell.run`。
- **占位符**：`{{var}}`、`{{input.x}}`、`{{step_id}}` / 步骤输出（见 engine resolver）。
- **控制流**：`if` / `repeat` / `parallel`（parallel 当前为顺序兼容模式）。

## 双模式

| 模式 | 入口 | 说明 |
|------|------|------|
| CLI | `corex run <name\|path>` | 进程内加载 registry + Pipeline |
| Daemon | `corex-daemon` / `corex daemon run` | Unix socket（默认 `<data-dir>/corex.sock`） |

数据目录默认由 `directories` 解析为平台 project data（fallback `.corex/`）。

## WASM 插件

见 [plugins/README.md](../plugins/README.md)。Host（`WasmPluginHost`）使用 wasmtime：async + component model + `WasiCtxBuilder`。WIT bindgen 完全接线前，无效/未完成的插件会被 discovery 记录并跳过。

## 配置

[`config/default.toml`](../config/default.toml)：

- `[daemon]` — socket / lock
- `[plugins]` — `plugin_dir`、禁用列表
- `[history]` — JSONL 执行历史
- `[logging]` / `[runtime]` — 日志与并行度、超时

## 构建

```bash
cargo build -p corex -p corex-daemon --release
cargo test --workspace
```

发布 ZIP（Windows CI）包含 `corex` + `corex-daemon`（+ `pdfium.dll` 若仍捆绑）。

## 相关文档

| 文档 | 说明 |
|------|------|
| [breaking-changes-v4.md](./breaking-changes-v4.md) | v4 破坏性变更 |
| [plugins/README.md](../plugins/README.md) | WASM 插件约定 |
| [tauri-integration.md](./tauri-integration.md) | Tauri sidecar（请改用 `corex-daemon`） |
