# Task Plan: Corex 企业级可组合快捷指令架构重构

## Goal
按 forge 架构方案将 corex 拆为 core/engine/registry/ipc/plugin-sdk + cli/daemon，保留项目名 corex；Daemon 二进制为 `corex-daemon`。

## Next Step
Windows CI 上验证 Named Pipe IPC（本环境为 Linux，已用 `#[cfg(windows)]` + interprocess 实现）

## Current Phase
P0–P5 完成（或近完成）；旧 monolith 已删除；Windows Named Pipe + CLI REPL 已落地

## Phases

### P0 — 骨架
- [x] Workspace: crates/{core,engine,registry,ipc,plugin-sdk} + bins/{cli,daemon}
- [x] corex-core: Value / Action / ExecutionContext / Error / Schema
- [x] corex-engine: Shortcut YAML + Pipeline
- [x] registry: builtins + feature gates
- [x] YAML → 执行 → 输出闭环
- **Status:** complete

### P1 — 控制流
- [x] if/else、repeat、parallel（parallel 支持并发 JoinSet + 上下文合并）
- [x] 变量解析 `{{var}}` / step / env / input / save_to
- **Status:** complete

### P2 — 双模式
- [x] corex CLI（run/list/actions/create/validate/daemon/repl）
- [x] corex-daemon + corex-ipc（Unix socket + Windows Named Pipe）
- **Status:** complete

### P3 — 插件生态
- [x] plugin-sdk WIT（corex:plugin-sdk@0.1.0 action）
- [x] wasmtime Engine（async + component model）+ WasiCtxBuilder host
- [x] discovery 扫描 `*.wasm`，load 失败则 log/skip
- [x] plugins/README.md
- **Status:** complete（bindgen 实例化待后续）

### P4 — 丰富内置
- [x] 计划 Action feature gate（shell/http/clipboard/notify/file/template/cron/keyring）
- [x] 现有业务模块迁为 Action（copy/scrub/shade/compression/… 已进 registry `full`）
- [x] 运行时 disabled 配置
- **Status:** complete

### P5 — 生产加固
- [x] ExecutionHistory JSONL + Pipeline/CLI/daemon 接线
- [x] history_smoke 测试
- [x] docs/architecture.md、breaking-changes-v4.md、README
- [x] CI：corex + corex-daemon（不再 corex-serve）
- [x] examples/tauri 引用更新；pipelines.yaml → examples/shortcuts/
- [x] 删除旧 monolith（`corex/`、`corex-core/`、`corex-serve/`、`corex-capture/`）；保留 `pdfium/`
- [x] Windows Named Pipe IPC（interprocess tokio；待 Windows CI 实机验证）
- [x] CLI REPL（`corex repl`）
- [x] add-module skill 改写为 v4 Action 流程
- **Status:** complete（Windows CI 验证仍为剩余项）

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Crate 前缀 `corex-*` | 用户要求 |
| Daemon = `corex-daemon` | 用户确认 |
| version 4.0.0 / edition 2021 | 破坏性 + 兼容 |
| Pipeline 用 `Arc<dyn ActionStore>` | 避免 engine↔registry 循环依赖 |
| Windows IPC = Named Pipe `\\.\pipe\corex` | 与旧习惯对齐；Unix 仍用 data-dir socket |
| 旧 crate 目录物理删除 | P4 完成且无 workspace 引用 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| orphan `impl ActionStore for Arc<T>` | 1 | 删除 Arc impl，依赖强制解引用/强制转换 |
| recursive async future | 1 | `Box::pin` 递归调用 |
| wasmtime-wasi 34 API under `p2` | 1 | 使用 `p2::{WasiCtx,WasiView,IoView,…}` |
| tracing 字段名 `display` 冲突 | 1 | 重命名局部变量 |
| JoinSet parallel 非 Send | 1→2 | 顺序兼容 → 后改为并发 + context merge |
| `as: i` 未映射 as_var | 1 | serde rename = "as" |
