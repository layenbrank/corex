# Task Plan: Corex 企业级可组合快捷指令架构重构

## Goal
按 forge 架构方案将 corex 拆为 core/engine/registry/ipc/plugin-sdk + cli/daemon，保留项目名 corex；Daemon 二进制为 `corex-daemon`。

## Next Step
P4：将旧业务模块迁为 Action；或按需清理旧 crate

## Current Phase
P0–P3 骨架已落地并通过编译/测试

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
- [x] plugin-sdk WIT
- [x] wasm_host + discovery 骨架（无 wasmtime）
- **Status:** complete (骨架)

### P4 — 丰富内置
- [x] 计划 Action feature gate（shell/http/clipboard/notify/file/template/cron/keyring）
- [ ] 现有模块迁为 Action
- [x] 运行时 disabled 配置
- **Status:** partial

### P5 — 生产加固
- [ ] tracing 全链路、历史、文档；清理旧 monolith
- **Status:** pending

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Crate 前缀 `corex-*` | 用户要求 |
| Daemon = `corex-daemon` | 用户确认 |
| version 4.0.0 / edition 2021 | 破坏性 + 兼容 |
| Pipeline 用 `Arc<dyn ActionStore>` | 避免 engine↔registry 循环依赖 |
| 旧 crate 暂留磁盘、移出 workspace | 用户要求 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| orphan `impl ActionStore for Arc<T>` | 1 | 删除 Arc impl，依赖强制解引用/强制转换 |
| recursive async future | 1 | `Box::pin` 递归调用 |
