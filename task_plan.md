# Task Plan: Corex 企业级可组合快捷指令架构重构

## Goal
按 forge 架构方案将 corex 拆为 core/engine/registry/ipc/plugin-sdk + cli/daemon，保留项目名 corex；Daemon 二进制为 `corex-daemon`。

## Next Step
P4：将旧业务模块迁为 Action（另一 agent）；确认可删旧 crate 后再清理

## Current Phase
P3 WASM host + P5 hardening/docs/CI 已完成

## Phases

### P0 — 骨架
- [x] Workspace: crates/{core,engine,registry,ipc,plugin-sdk} + bins/{cli,daemon}
- [x] corex-core: Value / Action / ExecutionContext / Error / Schema
- [x] corex-engine: Shortcut YAML + Pipeline
- [x] registry: builtins + feature gates
- [x] YAML → 执行 → 输出闭环
- **Status:** complete

### P1 — 控制流
- [x] if/else、repeat、parallel（parallel 为顺序兼容模式）
- [x] 变量解析 `{{var}}` / step / env / input / save_to
- **Status:** complete (骨架)

### P2 — 双模式
- [x] corex CLI（run/list/actions/create/validate/daemon）
- [x] corex-daemon + corex-ipc（Unix socket）
- **Status:** complete (骨架)

### P3 — 插件生态
- [x] plugin-sdk WIT（corex:plugin-sdk@0.1.0 action）
- [x] wasmtime Engine（async + component model）+ WasiCtxBuilder host
- [x] discovery 扫描 `*.wasm`，load 失败则 log/skip
- [x] plugins/README.md
- **Status:** complete（bindgen 实例化待后续）

### P4 — 丰富内置
- [x] 计划 Action feature gate（shell/http/clipboard/notify/file/template/cron/keyring）
- [ ] 现有模块迁为 Action（另一 agent；源文件可能已在工作区但未进 `full`）
- [x] 运行时 disabled 配置
- **Status:** partial

### P5 — 生产加固
- [x] ExecutionHistory JSONL + Pipeline/CLI/daemon 接线
- [x] history_smoke 测试
- [x] docs/architecture.md、breaking-changes-v4.md、README
- [x] CI：corex + corex-daemon（不再 corex-serve）
- [x] examples/tauri 引用更新；pipelines.yaml → examples/shortcuts/
- [ ] 删除旧 monolith 目录（P4 完成且无引用后再删）
- **Status:** complete（清理待 P4）

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Crate 前缀 `corex-*` | 用户要求 |
| Daemon = `corex-daemon` | 用户确认 |
| version 4.0.0 / edition 2021 | 破坏性 + 兼容 |
| Pipeline 用 `Arc<dyn ActionStore>` | 避免 engine↔registry 循环依赖 |
| 旧 crate 暂留磁盘、移出 workspace | 用户要求；P4 仍可能参考 |
| P4 feature 不进 `full` 直至可编译 | 避免挡住 P3/P5 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| orphan `impl ActionStore for Arc<T>` | 1 | 删除 Arc impl，依赖强制解引用/强制转换 |
| recursive async future | 1 | `Box::pin` 递归调用 |
| wasmtime-wasi 34 API under `p2` | 1 | 使用 `p2::{WasiCtx,WasiView,IoView,…}` |
| tracing 字段名 `display` 冲突 | 1 | 重命名局部变量 |
| JoinSet parallel 非 Send | 1 | 保持顺序兼容 parallel |
| `as: i` 未映射 as_var | 1 | serde rename = "as" |
