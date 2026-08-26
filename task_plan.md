# Task Plan: UI 自动化 + 交互式元素探测

## Goal

1. **已完成**：UI Selector/Sync/Session 三层内核、directive 可靠性、WeChat recipe、冒烟 YAML。
2. **当前焦点**：补齐 **交互式元素查找**（对标 Auto.js selector 探索），先分析、再最小 MVP。

## Current Phase

**Phase 6 — Interactive find（分析完成，待实现）**

## Phases

### Phase 1–5 — 可靠性内核（complete）

见历史记录：input defaults、process_launch GUI、`ui_kernel`、`ui_session`、文档、单测、Bugbot fixes。

### Phase 6 — Interactive element find（分析 → MVP）

| 子项 | 状态 | 说明 |
|------|------|------|
| Call-graph 梳理 directive → ui.element.* → ui_kernel | [x] | 见 `findings.md` |
| vs Auto.js 差距分析 | [x] | bounds/REPL/CLI probe 缺失 |
| Code review（ui refactor + smoke yaml） | [x] | 见 `findings.md` 严重度列表 |
| `docs/ui-automation.md` 扩展 Auto.js 对照 | [ ] | 待 doc PR：findOne/bounds/id/className |
| MVP 选型 | [x] | 推荐 `corex ui` 子命令 + REPL `find` |
| 实现 `corex ui find/list/windows` | [ ] | |
| REPL：`find` / `list-ui` / `scope` | [ ] | |
| `elem_to_map` 增加 bounds/enabled | [ ] | |
| `verify_closed` 语义修正 | [ ] | 窗口仍存在时应失败 |
| Windows 集成测 / 冒烟实机 | [ ] | |

**MVP 候选（择一或组合）：**

- **A. `corex ui find`** — 直接调 ActionRegistry + `ExecutionContext`，JSON 输出；可 `--hwnd` / `--title` / selector flags。
- **B. REPL 扩展** — `find name=…`、`list-elements`、`windows`；复用 `cmd_run` 同款 registry。
- **C. Daemon IPC invoke** — 已有 `invoke` 但无 session 串联；适合 Tauri，不适合交互探索首选。

### Phase 7 — 增强（post-MVP）

- [ ] selector builder（从 `ui.element.list` 生成 YAML `selectors[]`）
- [ ] `capture.screenshot` + `capture.ocr` 与 find 联动
- [ ] `ui_profile` directive input → runtime variables
- [ ] `corex directives diff`（data_dir vs examples）

## 企业门禁 Checklist

- [x] `permissions.ui` 文档化
- [x] `enterprise.toml` 禁用高风险 UI actions
- [ ] `validate --strict` 实机确认
- [ ] data_dir directive 与 examples 版本一致（运维）
- [ ] 交互式 find 纳入 enterprise 威胁模型评审（invoke / 屏幕坐标）

## Decisions

| Decision | Rationale |
|----------|-----------|
| 不新增 `wechat.*` | 微信仅 recipe 示例 |
| 交互 find 优先 CLI/REPL | 比新 directive 更快迭代 selector |
| `ui_kernel` 保持平台无关解析 | Windows UIA 仅在 `ui.rs` win 模块 |
| `config/corex.toml` 为运行时主配置 | `default.toml` 已弃用别名 |
| baseline `ui_max_selector_chain = 8` | `MAX_SELECTOR_CHAIN` + `ui_profile` |

## Errors Encountered

| Error | Resolution |
|-------|------------|
| findings 链长仍写 ≤5 | 本次分析更正为 baseline=8 |
| docs 路径 `%LOCALAPPDATA%` vs `%AppData%` | 代码实为 `%AppData%\corex\data` |
