# Task Plan: Corex 全阶段详细文档

## Goal

生成完整中文技术文档体系（主文档 + 三份专题文档），覆盖架构重构阶段 1–4、CLI/Daemon 用法、IPC 协议与 Tauri 接入，并修复缺失的 `ipc` example。

## Current Phase

Phase 5 — Delivery

## Phases

### Phase 1: Requirements & Discovery
- [x] 阅读计划与源码（serve、command、Cargo.toml、examples/tauri）
- [x] 记录调研结论到 findings.md
- **Status:** complete

### Phase 2: Planning Files
- [x] 创建 task_plan.md / findings.md / progress.md
- **Status:** complete

### Phase 3: 主文档与专题文档
- [x] docs/architecture-and-tauri-integration.md
- [x] docs/architecture.md
- [x] docs/ipc-protocol.md
- [x] docs/tauri-integration.md
- **Status:** complete

### Phase 4: 配套修复与索引
- [x] 恢复 corex-core/examples/ipc.rs
- [x] 更新 examples/tauri/README.md 交叉链接
- [x] 更新 README.md 文档索引
- **Status:** complete

### Phase 5: Delivery
- [x] 验收文档链接与 example 可编译
- **Status:** complete

## Key Questions

1. 文档组织方式？→ 主文档 + 三份专题文档（用户已确认）
2. ipc example 是否缺失？→ 是，需恢复

## Decisions Made

| Decision | Rationale |
|----------|-----------|
| 主文档作入口，细节下沉专题 | 避免重复，便于维护 |
| 不修改 plan 文件本身 | 用户明确要求 |
| 恢复 ipc.rs 而非仅文档引用 | Cargo.toml 已声明 example |

## Errors Encountered

| Error | Attempt | Resolution |
|-------|---------|------------|
| corex-core/examples/ipc.rs 不存在 | 1 | 按计划恢复文件 |

## Notes

- 阶段 4 Tauri 集成：示例代码已提供，实际 Tauri 项目集成由用户自行完成
- 阶段 5 基准测试：文档中标注为待办，不在本次范围
