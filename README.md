# CoreX

可组合的指令（Directive）/ Action 运行时：**YAML 定义流水线**，内置与 WASM 插件提供动作，**CLI 与 `corex-daemon` 共用同一引擎**。

当前主线为 **v4**（workspace `4.0.0`）。旧 Pipeline v3 / `corex-serve` 已移除；历史文档见 [`docs/archive/`](docs/archive/)。

## Workspace 布局

| 路径 | 包名 | 说明 |
|------|------|------|
| `crates/core` | `corex-core` | Value / Action / ExecutionContext |
| `crates/engine` | `corex-engine` | Directive YAML、Pipeline、历史 |
| `crates/registry` | `corex-registry` | 内置 Action + WASM host |
| `crates/ipc` | `corex-ipc` | NDJSON；Unix socket / Named Pipe |
| `crates/plugin-sdk` | `corex-plugin-sdk` | WIT 契约 |
| `bins/cli` | `corex` | CLI |
| `bins/daemon` | `corex-daemon` | 后台 Daemon |
| `config/corex.toml` | — | 运行时配置 |
| `examples/directives/` | — | Directive 示例 |
| `pdfium/` | `pdfium` | 可选 native DLL（morph） |

## 快速开始

```bash
# 构建
cargo build -p corex -p corex-daemon

# 运行示例 Directive
cargo run -p corex -- run examples/directives/hello.yaml
cargo run -p corex -- run examples/directives/hello.yaml --input who=Corex

# 列出 / 校验 / 创建
cargo run -p corex -- list
cargo run -p corex -- actions
cargo run -p corex -- validate examples/directives/hello.yaml
cargo run -p corex -- create my-Directive

# 交互 REPL
cargo run -p corex -- repl

# Daemon（Unix: <data-dir>/corex.sock ；Windows: \\.\pipe\corex）
cargo run -p corex-daemon
# 或
cargo run -p corex -- daemon run
cargo run -p corex -- daemon status
```

示例 `hello.yaml` 会渲染问候语并写入 `/tmp/corex-hello.txt`（见 [`examples/directives/hello.yaml`](examples/directives/hello.yaml)）。另有 [`control-flow.yaml`](examples/directives/control-flow.yaml)、[`copy-demo.yaml`](examples/directives/copy-demo.yaml)。

## CLI 命令

| 命令 | 说明 |
|------|------|
| `corex run <name\|path>` | 执行 Directive（`--input KEY=VALUE`） |
| `corex list` | 列出 Directive |
| `corex actions` | 列出已注册 Action |
| `corex create <name>` | 创建 Directive 脚手架 |
| `corex validate <path>` | 校验 YAML |
| `corex repl` | 交互：`help` / `actions` / `list` / `run` / `quit` |
| `corex daemon` | `start` / `stop` / `status` / `run` |

全局可用 `-v` / `-vv` 提高日志级别；`--dir` 覆盖 directives 目录。

### 独立 Binary

| Binary | 说明 |
|--------|------|
| `corex` | CLI |
| `corex-daemon` | IPC Daemon（Tauri / 宿主 sidecar） |

```bash
cargo build -p corex -p corex-daemon --release
```

IPC 默认：Unix `<data-dir>/corex.sock`；Windows `\\.\pipe\corex`。鉴权 token：`COREX_TOKEN` 或 `<data-dir>/token`（见 [docs/ipc-protocol.md](docs/ipc-protocol.md)）。

### GitHub Release（Windows x64）

打 `v*` SemVer 标签（或 `workflow_dispatch`）会发布 `corex-{tag}-windows-x64.zip`，通常包含 `corex.exe`、`corex-daemon.exe`，以及可选的 `pdfium.dll`（morph）。

## 文档

| 文档 | 说明 |
|------|------|
| [docs/architecture.md](docs/architecture.md) | v4 架构与配置段 |
| [docs/ipc-protocol.md](docs/ipc-protocol.md) | NDJSON 协议、token、端点 |
| [docs/directive-yaml.md](docs/directive-yaml.md) | Directive DSL |
| [docs/actions.md](docs/actions.md) | 内置 Action ID 表 |
| [docs/breaking-changes-v4.md](docs/breaking-changes-v4.md) | v4 破坏性变更 |
| [docs/tauri-integration.md](docs/tauri-integration.md) | Tauri + `corex-daemon` |
| [plugins/README.md](plugins/README.md) | WASM 插件 |
| [examples/tauri/](examples/tauri/) | Tauri 示例代码 |
| [docs/archive/](docs/archive/) | ≤v3 历史文档 |

## 迁移提示（v3 → v4）

- 二进制：`corex-serve` → **`corex-daemon`**。
- YAML：Pipeline v3 → **Directive**（`action` + `{{ }}`）；旧样例在 [`examples/legacy/`](examples/legacy/)。
- 旧 `corex copy` / `pipeline` / `watch` 子命令已移除；用 Action（如 `copy.run`）写进 Directive，或 IPC `invoke`。
- **morph / pdfium**：以 Action `morph.*` 调用；发布包若捆绑 `pdfium.dll`，需与二进制同目录；开发可用 `scripts/download-pdfium.ps1`（若存在）。

## 许可证 / 贡献

见仓库内贡献与 CI 工作流（`.github/`）。问题与 PR 请对照 [docs/architecture.md](docs/architecture.md) 与 [docs/actions.md](docs/actions.md)。
