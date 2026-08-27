# Task Plan: Code Review 修复

## Goal
按审查意见修复企业门禁、Win11 desktop、pick UX、tree 合并、`--class`、redact；更新文档与配置。

## Current Phase
**Complete**

## Phases

### Phase A — 合并前必修（审查 1–4）
- [x] `check_probe_allowed` 尊重 `plugins.disabled`
- [x] CLI 加载并强制 `strict_permissions`
- [x] desktop / point / pick 独立 audit/gate action_id；enterprise.toml 同步
- [x] `find_desktop_hwnd`：Progman + WorkerW/`SHELLDLL_DefView` fallback

### Phase B — 中优行为（审查 5–7、10）
- [x] pick scope 外左键 stderr 提示
- [x] `element get --class`
- [x] `node_key` 纳入 bounds
- [x] `--redact` 打码 `automation_id`

### Phase C — 文档 / 测试 / 低优
- [x] 文档对齐门禁与命令
- [x] Windows：`probe_scope_required` 取消 ignore
- [x] 单测覆盖；commit + push
- [x] 跳过：pick USERDATA 大 refactor（#8）；全量 Windows 实机 CI（#9）

## Errors Encountered
| Error | Resolution |
|-------|------------|
| （待填） | |
