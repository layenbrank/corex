# Task Plan: corex crates/bins 架构逻辑优化

## Goal
对 `crates/`、`bins/cli`、`bins/daemon`、`config/`、`pdfium/`、`tests/` 做可维护的架构级代码逻辑优化：合并双轨、去补丁叠层、统一错误/审计控制流。

## Next Step
交付中文结构化报告。

## Current Phase
Phase 5

## Phases

### Phase 1: 调研
- **Status:** complete

### Phase 2: 合并双轨 / 删旧路径
- [x] typed audit/history（工作区 WIP + 收尾）
- [x] UI 码/selector 并入 ActionError
- [x] pipeline on_error 控制流；prefer_branch_err
- [x] ActionStore::find_action / actions
- **Status:** complete

### Phase 3: 模块边界与控制流
- [x] AuditEntry 收敛到 from_engine/from_action → failure(typed)
- [x] cli/daemon 已对齐 typed audit
- **Status:** complete

### Phase 4: 测试与验证
- [x] cargo check（core/engine/registry/cli/daemon）
- [x] cargo test core error + engine lib + parallel/permissions + daemon
- **Status:** complete

### Phase 5: 交付报告
- [x] 中文结构化报告
- **Status:** complete

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| UI bracket 解析进 core | 单一来源，删 audit 二次解析 |
| find_action / actions | 命名约束；调用点少 |
| 不铺 deny/pre-commit | 与逻辑优化无关 |
| 不改 pdfium/config | 无架构债 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
| session-catchup.py 缺失 | 1 | 跳过 |
| package corex-cli 不存在 | 1 | 包名是 `corex` |
| nextest 未安装 | 1 | 用 cargo test |
