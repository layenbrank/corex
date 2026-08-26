# Findings — UI 自动化可靠性

## 根因（三层叠加）

| 层 | 现象 | 根因 |
|----|------|------|
| Input | `input.wechat_path` 未定义 | data_dir 旧副本无 `default`；engine 不把空串当缺失 |
| Launch | 已登录仍弹登录壳 | `shell.run` 同步 `output()`；重复 spawn GUI |
| UI | 只搜索不发消息 | 第一个「微信」HWND 可能是登录壳；无元素级登录分支 |

## WeChat 4.x 登录 UI

- 窗口标题：**微信**（登录壳与主界面相同）
- 登录探测：`name=进入微信` + `control_type=Button`（UIA 元素，非标题）
- 手机确认：**人工环节**；PC 侧 `ui.element.wait state=absent` + 超时 `ui_login_pending`

## 架构（已实现）

```
Recipe (wechat-send-message.yaml)
  → Pipeline::apply_input_defaults
  → shell.run → process_launch::launch(detach, if_running_window)
  → ui.window.* / ui.element.* → ui_kernel (WindowQuery, ElementSelector)
  → ExecutionContext.ui_session (scope_hwnd)
  → AuditEntry (ui_phase, error_code, selector_hint)
```

## Call-graph 验证（2026-08-26）

| 符号 | 调用方 | 旧路径 |
|------|--------|--------|
| `apply_input_defaults` | 仅 `pipeline.rs:64` | 无双轨 |
| `launch_spec_from_command_params` | `shell.rs`, `exec.rs` | 无双轨 |
| `window_query_from_params` | 所有 `ui.window.*` | `find_hwnd_by_title` 已移除 |
| `selector_chain_from_params` | exists/wait/click | 链长 ≤5 |

## Auto.js 对照

| Auto.js | corex |
|---------|-------|
| `waitFor()` | `ui.element.wait state=present` |
| `!exists()` 轮询 | `state=absent` |
| `clickable()` | `state=enabled` + safe click |
| `selector()` 多属性 | `ElementSelector` + `selectors[]` |

## Future work

- `ui_profile` input → variables 映射（fast/patient 预设）
- `corex directives diff` CLI（data_dir 副本对比）
- Windows nightly：`if_running_window: skip` 集成测

## Out of scope

- 手机端自动化 / OCR 扫码
- `wechat.send` / `ui.app.ensure`
- macOS AX / Linux AT-SPI（见 cross-platform-backends.md）
