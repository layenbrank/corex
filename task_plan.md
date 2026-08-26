# Task Plan: UI Inspector 企业级实现

## Goal
双模式 UI Inspector：CLI 嵌套命令（window/element）+ scope 硬约束 + pick 全局输入修复 + Tauri 骨架 + 企业门禁与测试。

## Current Phase
**Complete**

## Phases

### Phase 1 — CLI 嵌套重命名 + scope
- [x] `window list` / `window desktop` / `element tree|get|point|pick`
- [x] `element tree/get` 强制 `--hwnd|--title`
- [x] `probe_desktop_icons` 独立入口

### Phase 2 — 输出增强
- [x] `ancestors[]`、`--format tree`、`suggest_selectors` class 回退
- [x] `--redact`

### Phase 3 — pick 输入修复
- [x] 删 layered overlay 点击捕获
- [x] timer + `GetAsyncKeyState(LBUTTON)` + 隐藏 msg 窗口
- [x] 终端置底

### Phase 4 — 企业门禁
- [x] `disabled_actions` 检查
- [x] `audit.jsonl` ui.probe 事件

### Phase 5 — Tauri + 文档 + 测试
- [x] Inspector 骨架 + IPC helpers
- [x] docs 更新
- [x] 单测 + Windows 集成测 stub

## Errors Encountered
| Error | Resolution |
|-------|------------|
| edition2024 需 Cargo 1.85+ | rustup stable 1.98 |
| daemon 缺 Write import | 补 `use std::io::Write` |
