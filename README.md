# Corex

可组合的**指令（Directive）/ Action** 运行时：用 **YAML** 定义自动化流水线，内置与 WASM 插件提供动作，**CLI** 与 **`corex-daemon`** 共用同一引擎。

**当前版本：v5**（workspace `5.0.0`）

---

## 快速开始

```powershell
cargo build -p corex -p corex-daemon
corex run hello
corex run hello -i who=Corex
corex schedule
corex actions
```

👉 完整入门：[docs/guide/快速开始.md](docs/guide/快速开始.md)

---

## 文档（简体中文）

**[📖 文档中心 — docs/README.md](docs/README.md)**（推荐入口）

| 分类 | 文档 |
|------|------|
| 入门 | [快速开始](docs/guide/快速开始.md) · [指令与输入配置](docs/guide/指令与输入配置.md) |
| 示例 | [examples/directives/README.md](examples/directives/README.md) |
| 接入/SDK | [接入总览](docs/integration/接入总览.md) · [IPC](docs/integration/IPC接入指南.md) · [Rust 嵌入](docs/integration/Rust嵌入指南.md) · [WASM 插件](docs/integration/WASM插件开发.md) · [Tauri](docs/integration/Tauri接入指南.md) |
| 参考 | [Directive YAML](docs/directive-yaml.md) · [内置动作](docs/actions.md) · [架构](docs/architecture.md) |
| 配置 | [运行时配置](docs/guide/运行时配置.md) · [config/corex.toml](config/corex.toml) |
| 运维 | [企业部署](docs/enterprise-deploy.md) · [合规](docs/compliance.md) |

---

## Workspace 布局

| 路径 | 说明 |
|------|------|
| `crates/core` | `corex-core` — Value / Action / ExecutionContext |
| `crates/engine` | `corex-engine` — Directive、Pipeline、解析器 |
| `crates/registry` | `corex-registry` — 内置 Action、WASM host |
| `crates/ipc` | `corex-ipc` — NDJSON 协议 |
| `crates/plugin-sdk` | WASM 插件 WIT 契约 |
| `bins/cli` | `corex` CLI |
| `bins/daemon` | `corex-daemon` IPC 服务 |
| `examples/directives/` | 可运行 YAML 示例 |
| `examples/tauri/` | Tauri sidecar 接入示例 |

---

## CLI 命令

| 命令 | 说明 |
|------|------|
| `corex run <名称\|路径>` | 执行指令（`-i KEY=VALUE`） |
| `corex schedule` | 列出指令 |
| `corex watch …` / `corex cron …` | 监听 / 定时守护 |
| `corex actions` | 列出 Action |
| `corex validate <path>` | 校验 YAML |
| `corex create / edit / repl` | 脚手架 / 编辑 / REPL |
| `corex daemon start\|stop\|status\|run` | Daemon 管理 |
| `corex ui ...` | Windows UI 探测（可选） |

---

## 三种集成方式

| 方式 | 场景 | 文档 |
|------|------|------|
| CLI | 脚本、CI | [快速开始](docs/guide/快速开始.md) |
| Daemon + IPC | Tauri、多进程客户端 | [IPC 接入指南](docs/integration/IPC接入指南.md) |
| Rust 嵌入 | 同进程集成 | [Rust 嵌入指南](docs/integration/Rust嵌入指南.md) |

---

## 迁移与归档

- v5 变更：[docs/breaking-changes-v5.md](docs/breaking-changes-v5.md)
- v4 变更：[docs/breaking-changes-v4.md](docs/breaking-changes-v4.md)
- v3 及更早：[docs/archive/](docs/archive/)

---

## 许可证

见仓库贡献说明与 CI 配置（`.github/`）。
