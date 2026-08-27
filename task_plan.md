# Task Plan: 企业架构加固（P0–P3）

## Goal
落实架构评分建议：对齐企业 preset/文档、exec/shell 路径沙箱、corex-core 统一运行时门禁、history 错误收敛与运维文档。

## Next Step
Phase 5：commit / push / 更新 PR

## Current Phase
Phase 5

## Phases

### Phase 0 — 规划文件重置
- [x] 重写 task_plan.md / findings.md / progress.md
- **Status:** complete

### Phase 1 — P0：Preset / 文档对齐
- [x] 同步 enterprise-deploy.md ← enterprise.toml
- [x] 补禁 capture.monitors / keyring.* / scan.os
- [x] 更新 architecture.md 配置表
- [x] 轻触 compliance / threat-model
- **Status:** complete

### Phase 2 — P1：路径沙箱
- [x] exec.run：script + cwd → confine_path
- [x] shell.run：cwd → confine_path
- [x] 单测 + breaking-changes-v5 说明
- **Status:** complete

### Phase 3 — P2：统一运行时门禁
- [x] PermissionKind + permission_kind_for → corex-core
- [x] check_runtime_allowed；daemon / ui_probe 共用
- **Status:** complete

### Phase 4 — P3：History + 最小构建文档
- [x] history error sanitize
- [x] 最小企业构建 + CLI 信任边界文档
- **Status:** complete

### Phase 5 — 验证与交付
- [x] cargo test --workspace --locked
- [x] commit / push / 更新 PR
- **Status:** complete

## Next Step
等待 CI 结果；无更多实现项。

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| 本轮不做 cron / WASM / pick USERDATA | 聚焦评分 P0–P3 |
| 门禁下沉 corex-core | 避免 registry↔engine 循环依赖 |
| shell.command 不 confine | PATH 查找设计限制 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| Windows CI: exec roots-inside bat → cmd “path not found” | 1 | `canonicalize` 产生 `\\?\`；`for_external_process` 剥离后交给 launch |
