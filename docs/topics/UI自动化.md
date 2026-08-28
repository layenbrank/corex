# UI 自动化（corex 内置）

面向 Windows（UIAutomation + Win32）的 `ui.*` 企业指南。macOS/Linux 后端见 [cross-platform-backends.md](跨平台后端.md)。

## 原则

1. **元素同步优先于固定休眠** — 优先 `ui.element.wait`（`present` / `absent` / `enabled`），少用 `ui.wait`。企业可通过 `[runtime].ui_max_settle_ms` 限制累计 settle 时间。
2. **多属性选择器** — 组合 `name`、`automation_id`、`control_type`；应用换版时用 `selectors: [...]` 回退链。链长受 `[runtime].ui_max_selector_chain` 限制（`ui_profile = "baseline"` 为 **8**；`fast` = 5，`patient` = 12）。
3. **会话作用域** — `ui.window.find|wait|focus` 后，`ExecutionContext.ui_session` 缓存 `scope_hwnd`；后续步骤可省略 `title_contains`。
4. **人在回路** — 需手机确认的流程（如微信 4.x「进入微信」）必须用 `ui.element.wait`（`state: absent`）并给足 `timeout_ms`；失败返回 `[ui_login_pending]`。

## Auto.js / AutoX 对照

| Auto.js | corex 指令 | `corex ui` CLI |
|---------|------------|----------------|
| `selector().findOne(timeout)` | `ui.element.find` | `corex ui element get --name "…"` |
| `exists()` | `ui.element.exists` | （`element get` + 查 JSON） |
| `waitFor()` | `ui.element.wait` `state: present` | — |
| `clickable()` + click | `ui.element.click` `safe: true` | JSON 字段 `clickable` |
| `bounds()` | — | JSON 字段 `bounds: {x,y,width,height}` |
| `id()` / `className()` | `automation_id` / `class` | `element get` / `element pick` 输出 |
| 布局分析 / 控件树 | `ui.element.list` | `corex ui element tree` |
| 桌面图标 | — | `corex ui window desktop` |
| 点击选控件（Inspect） | — | `corex ui element pick`（CLI 应急）；Tauri Inspector 为主 UX |
| `sleep()` | **避免** — 用 `ui.element.wait`；兜底 `ui.wait` | — |

## 交互探测（`corex ui`）

仅 Windows。**Tauri Inspector**（[Tauri接入指南](../integration/Tauri接入指南.md)）提供树形 UI；CLI 适合脚本/CI。

```powershell
corex ui window list
corex ui window desktop
corex ui element tree --title "无标题 - Notepad" --depth 4
corex ui element tree --title "无标题 - Notepad" --format tree
corex ui element get --title "无标题 - Notepad" --control-type document
corex ui element get --title "记事本" --class Edit
corex ui element point --x 640 --y 480
corex ui element pick --copy-yaml
```

- `element tree` / `element get` **必须** `--hwnd` 或 `--title`（否则 `ui_scope_required`）
- `element pick` 用全局左键确认（FlaUI 四边框高亮 + `GetAsyncKeyState`），不依赖 overlay 焦点；scope 外点击会 stderr 提示
- 输出含 `ancestors[]`、`selectors_yaml`；可选 `--redact` 打码 `name` / `automation_id`（含 ancestors）
- 企业门禁与 daemon 对齐：`plugins.disabled`、`disabled_actions`、`[runtime].strict_permissions`；probe 写入 `audit.jsonl`（`ui.probe`）
- 审计 / 门禁 action id：`ui.window.list` / `ui.window.desktop` / `ui.element.list` / `ui.element.find` / `ui.element.point` / `ui.element.pick`

推荐流程：window list → element tree/get → 粘贴 YAML → `corex run ui-smoke-notepad` 验证。

## 结构化错误

| Code | 含义 |
|------|------|
| `ui_selector_not_found` | 选择器链未匹配到元素 |
| `ui_not_clickable` | 找到元素但禁用/不可见 |
| `ui_wrong_window` | 作用域窗口缺失 |
| `ui_login_pending` | 登录 UI 仍在（手机未确认） |
| `ui_sync_timeout` | 元素/窗口等待超时 |
| `ui_scope_required` | Probe 缺少 `--hwnd` / `--title` |
| `ui_desktop_not_found` | 桌面 Shell 窗口未找到 |

适用时，`audit.jsonl` 会包含 `ui_phase` 与 `error_code`。

## 启动 GUI 应用

使用 `shell.run`：

```yaml
params:
  command: "C:\\Path\\App.exe"
  wait: detach
  if_running: skip
  if_running_window:
    title_contains: "MyApp"
    prefer_largest: true
```

## 指令输入

YAML 中带 `default` 的可选输入：调用方省略该键，**或**传入空字符串时，会应用默认值。升级后请将 `%AppData%\corex\data\directives\` 与 `examples/directives/` 同步。

## 冒烟测试

[`examples/directives/ui-smoke-notepad.yaml`](../../examples/directives/ui-smoke-notepad.yaml) 在 **Win11 记事本**（简体中文）上覆盖 13 个 `ui.*` Action。

Win11 记事本注意：

- 标签显示 **无标题**；顶层 HWND 标题多为 **无标题 - Notepad**
- 默认 `window_hint` 为 **无标题**（不是 `Notepad`）
- 菜单：**文件 / 编辑 / 查看**；编辑区为 UIA `Document`
- 富文本工具栏按钮跨版本脆弱 — 优先对 `Document` 做 `ui.element.click` + `ui.type`

```powershell
corex run ui-smoke-notepad
corex run ui-smoke-notepad -i close_after=false   # 保持窗口以便检查
```

拉取代码后请重建 corex（`ui.key` 需支持 `End`/`Home`）。

升级后同步 `%AppData%\corex\data\directives\`。

## 运行时 UI profile

在 `config/corex.toml` 或 `<data_dir>/config.toml` 中配置：

```toml
[runtime]
ui_profile = "patient"          # baseline | fast | patient
# ui_max_selector_chain = 12    # 可选覆盖
# ui_max_settle_ms = 0          # 限制累计 ui.wait ms（fast 预设 = 2000）
```

| Profile | `ui_max_selector_chain` | `ui_max_settle_ms` |
|---------|-------------------------|--------------------|
| `baseline` | 8 | 0（不限制） |
| `fast` | 5 | 2000 |
| `patient` | 12 | 0 |

旧值 `ui_profile = "default"` 视为 `baseline` 别名。

Win11 记事本：输入后标签从「无标题」变为首行内容。起始窗口步骤用 `window_hint`；关闭后校验应用 `hwnd: '{{target_window.hwnd}}'`，勿依赖标题。

## 相关文档

- [actions.md](../reference/内置Action.md)
- [compliance.md](../ops/合规说明.md)
- [enterprise-deploy.md](../ops/企业部署.md)
- [examples/directives/wechat-send-message.yaml](../../examples/directives/wechat-send-message.yaml)
- [examples/directives/ui-smoke-notepad.yaml](../../examples/directives/ui-smoke-notepad.yaml)
