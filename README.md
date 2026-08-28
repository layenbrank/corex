# Corex

可组合的**指令（Directive）/ Action** 运行时：用 YAML 定义流水线，CLI 与 `corex-daemon` 共用同一引擎。

**当前版本：v5**（workspace `5.0.0`）

## 快速开始

完整说明见 → [docs/guide/快速开始.md](docs/guide/快速开始.md)

```powershell
cargo build -p corex -p corex-daemon
corex run hello
corex run hello -i who=Corex
corex schedule
corex actions
```

## 常用命令

| 命令 | 说明 |
|------|------|
| `corex run <名称\|路径>` | 执行指令（`-i KEY=VALUE`） |
| `corex schedule` | 列出指令 |
| `corex watch …` / `corex cron …` | 文件监听 / 定时守护 |
| `corex actions` | 列出 Action |
| `corex validate <path>` | 校验 YAML |
| `corex create` / `edit` / `repl` | 脚手架 / 编辑 / REPL |
| `corex daemon start\|stop\|status\|run` | Daemon 管理 |
| `corex ui ...` | Windows UI 探测 |

## 文档

→ **[docs/README.md](docs/README.md)**（分类索引，推荐从这里找）

| 分类 | 入口 |
|------|------|
| 入门 | [快速开始](docs/guide/快速开始.md) · [指令与输入](docs/guide/指令与输入配置.md) |
| 参考 | [指令 YAML](docs/reference/指令YAML.md) · [内置 Action](docs/reference/内置Action.md) · [架构](docs/reference/架构.md) |
| 接入 | [接入总览](docs/integration/接入总览.md) · [IPC](docs/integration/IPC接入指南.md) · [Tauri](docs/integration/Tauri接入指南.md) |
| 示例 | [directives](examples/directives/README.md) · [actions](examples/actions/README.md) |
| 运维 | [企业部署](docs/ops/企业部署.md) · [合规](docs/ops/合规说明.md) |
| 变更 | [v5](docs/changelog/破坏性变更-v5.md) · [v4](docs/changelog/破坏性变更-v4.md) · [archive](docs/archive/) |

## Workspace

| 路径 | 说明 |
|------|------|
| `crates/core` | Value / Action / ExecutionContext |
| `crates/engine` | Directive、Pipeline、解析器 |
| `crates/registry` | 内置 Action、WASM host |
| `crates/ipc` | NDJSON 协议 |
| `bins/cli` · `bins/daemon` | `corex` / `corex-daemon` |
| `examples/directives/` · `examples/actions/` | 可运行 YAML |
| `examples/tauri/` | Tauri sidecar 示例 |

## 三种集成方式

| 方式 | 场景 | 文档 |
|------|------|------|
| CLI | 脚本、CI | [快速开始](docs/guide/快速开始.md) |
| Daemon + IPC | Tauri、多进程 | [IPC 接入](docs/integration/IPC接入指南.md) |
| Rust 嵌入 | 同进程 | [Rust 嵌入](docs/integration/Rust嵌入指南.md) |

## 许可证

见仓库贡献说明与 CI（`.github/`）。
