# Task Plan: UI 自动化可靠性修复（通用 Action 层）

## Goal
修复 input 默认值、GUI 启动语义、UI Selector/Sync/Session 三层内核；WeChat 4.x 登录仅作 recipe 示例。

## Current Phase
**Complete**

## Phases

### Phase 1 — Engine input defaults
- [x] `apply_input_defaults`：空值/Null/空白视为未提供
- [x] `pipeline.rs` 在 `execute()` 开头调用
- [x] 测试 `crates/engine/tests/input_defaults.rs`
- **Status:** complete

### Phase 2 — process_launch GUI
- [x] `wait: sync | detach`
- [x] `if_running: launch | skip | fail`
- [x] `if_running_window` + `prefer_largest`
- [x] shell/exec 透传
- [x] Bugbot fix：`if_running_window` 未命中时不按进程名 skip
- **Status:** complete

### Phase 3 — UI 内核
- [x] `ui_kernel.rs`：`WindowQuery` / `ElementSelector` / `selectors[]` 回退链
- [x] `ExecutionContext.ui_session`
- [x] `ui.element.exists` / `ui.element.wait` (present|absent|enabled)
- [x] `ui.element.click` safe 模式
- [x] `ui.wait` 兜底 + `ui_max_settle_ms`
- [x] audit `ui_phase` / `error_code` / `selector_hint`
- [x] Bugbot fix：`prefer_largest` 不绑定 stale session hwnd
- **Status:** complete

### Phase 4 — WeChat recipe + 文档
- [x] `wechat-send-message.yaml` v0.5
- [x] `docs/ui-automation.md`
- [x] `docs/compliance.md` 人工检查点
- **Status:** complete

### Phase 5 — 测试与验收
- [x] 单测（input_defaults / process_launch / ui_kernel / audit）
- [x] `example_directives_validate`
- [x] Bugbot 审查 + Must-Fix 修复
- [x] `cargo test --workspace` 全绿
- [ ] Windows 实机清单（手工）
- **Status:** complete（代码/单测）；实机待运维

## 企业门禁 Checklist
- [x] `permissions.ui` 文档化
- [x] `enterprise.toml` 禁用高风险 UI actions
- [x] `docs/compliance.md` 人工检查点
- [ ] `validate --strict` 实机确认
- [ ] data_dir directive 与 examples 版本一致（运维）

## Decisions
| Decision | Rationale |
|----------|-----------|
| 不新增 `wechat.*` | 微信仅 recipe 示例 |
| 4.x 登录靠 UIA 探测「进入微信」 | 标题同为「微信」 |
| 跳过 `ui_profile` input | future work |
| CLI `-i` 用 `Value::from_cli_literal` | 修复 `auto_login=false` 字符串 truthy |

## Errors Encountered
| Error | Resolution |
|-------|------------|
| session-catchup.py 路径不存在 | git diff + grep 手动恢复 |
| Bugbot: process skip 误触 | `if_running_window` 未命中时不 fallback 进程名 |
| Bugbot: stale ui_session hwnd | `prefer_largest` 忽略 session hwnd |
| Bugbot: CLI bool 字符串 | `Value::from_cli_literal` |
