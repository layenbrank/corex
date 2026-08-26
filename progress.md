# Progress Log

## Session: 2026-08-26 — UI 自动化验证收尾

### Done
- [x] 重写 planning 文件（task_plan / findings / progress）
- [x] call-graph 验证：无双轨；`find_hwnd_by_title` 已移除
- [x] audit `selector_hint` + `ActionError::ui_with_hint`
- [x] `docs/compliance.md` Human-in-the-loop checkpoints
- [x] 移除 wasmtime deprecated `async_support`
- [x] Bugbot 3× Must-Fix 已修复
- [x] `cargo test --workspace` 全绿（88 tests）

### Bugbot Must-Fix 修复
1. **process_launch**：配置 `if_running_window` 且窗口未命中时，不再按进程名 skip
2. **ui_kernel**：`prefer_largest: true` 时忽略 `ui_session.scope_hwnd`，重新枚举选最大窗
3. **CLI**：`-i auto_login=false` 解析为 `Value::Bool(false)`（`Value::from_cli_literal`）

### Validate
```bash
cargo test --workspace
cargo test -p corex-engine example_directives_validate
```
结果：**全部通过**

### data_dir 同步（运维必做）
```
%LOCALAPPDATA%\corex\directives\wechat-send-message.yaml
```
必须与 `examples/directives/wechat-send-message.yaml` 同步（含 `default` inputs）。

### Windows 实机验收清单
- [ ] 1. 不传 `wechat_path` → 无 `input.wechat_path` 错误
- [ ] 2. 已登录 + 主窗在 → skip launch，完整发消息
- [ ] 3. 误 spawn 登录壳 → 自动点「进入微信」→ 手机确认 → 发消息
- [ ] 4. 冷启动未登录 → 登录壳 → 点按钮 → 等手机确认 → 发消息
- [ ] 5. 手机未确认 → `ui_login_pending` 超时，明确失败
- [ ] 6. audit 含 `ui_phase` + `error_code` + `selector_hint`
- [ ] 7. selector 回退链有效
- [ ] 8. `enterprise.toml` + `ui_max_settle_ms=500` 无大量 `ui.wait` 仍通过

## 5-Question Reboot Check
| Question | Answer |
|----------|--------|
| Where am I? | All plan todos complete |
| Where am I going? | Windows 实机验收（运维） |
| What's the goal? | 企业级 UI 自动化内核可靠 |
| What have I learned? | 托盘常驻 + 无窗口时不应 process skip |
| What have I done? | 验证收尾 + Bugbot fixes + 全量测试 |
