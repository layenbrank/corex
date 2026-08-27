# UI automation (corex builtins)

Enterprise guide for `ui.*` actions on Windows (UIAutomation + Win32). macOS/Linux
backends are planned; see [cross-platform-backends.md](./cross-platform-backends.md).

## Principles

1. **Element sync over fixed sleep** — prefer `ui.element.wait` (`present` / `absent` /
   `enabled`) over `ui.wait`. Use `ui.wait` only as a last resort; enterprise config may
   cap total settle time via `[runtime].ui_max_settle_ms`.
2. **Multi-attribute selectors** — combine `name`, `automation_id`, `control_type`; use
   `selectors: [...]` fallback chains when apps change between versions. Chain length is
   capped by `[runtime].ui_max_selector_chain` (baseline **8** via `ui_profile = "baseline"`;
   `fast` = 5, `patient` = 12).
3. **Session scope** — after `ui.window.find|wait|focus`, `ExecutionContext.ui_session`
   caches `scope_hwnd`; later steps may omit `title_contains`.
4. **Human checkpoints** — flows that require phone approval (e.g. WeChat 4.x「进入微信」)
   must use `ui.element.wait` with `state: absent` and a generous `timeout_ms`; failures
   return `[ui_login_pending]`.

## Auto.js / AutoX mapping

| Auto.js | corex directive | `corex ui` CLI |
|---------|-----------------|----------------|
| `selector().findOne(timeout)` | `ui.element.find` | `corex ui element get --name "…"` |
| `exists()` | `ui.element.exists` | (use `element get` + check JSON) |
| `waitFor()` | `ui.element.wait` `state: present` | — |
| `clickable()` + click | `ui.element.click` `safe: true` | JSON 字段 `clickable` |
| `bounds()` | — | JSON 字段 `bounds: {x,y,width,height}` |
| `id()` / `className()` | `automation_id` / `class` | `element get` / `element pick` 输出 |
| 布局分析 / 控件树 | `ui.element.list` | `corex ui element tree` |
| 桌面图标 | — | `corex ui window desktop` |
| 点击选控件（Inspect） | — | `corex ui element pick`（CLI 应急）；Tauri Inspector 为主 UX |
| `sleep()` | **avoid** — `ui.element.wait`; fallback `ui.wait` | — |

## Interactive probe (`corex ui`)

Windows-only CLI for exploring selectors. **Tauri Inspector**（[tauri-integration.md](./tauri-integration.md)）提供树形 UI；CLI 适合脚本/CI。

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
- `element pick` 使用全局左键确认（FlaUI 四边框高亮 + `GetAsyncKeyState`），不依赖 overlay 焦点；scope 外点击会 stderr 提示
- 输出含 `ancestors[]`、`selectors_yaml`；可选 `--redact` 打码 `name` / `automation_id`（含 ancestors）
- 企业门禁与 daemon 对齐：`plugins.disabled`、`disabled_actions`、`[runtime].strict_permissions`；probe 事件写入 `audit.jsonl`（`ui.probe`）
- 审计 / 门禁 action id：`ui.window.list` / `ui.window.desktop` / `ui.element.list` / `ui.element.find` / `ui.element.point` / `ui.element.pick`

Workflow：window list → element tree/get → 粘贴 YAML → `corex run ui-smoke-notepad` 验证。

## Structured errors

| Code | Meaning |
|------|---------|
| `ui_selector_not_found` | No element matched selector chain |
| `ui_not_clickable` | Element found but disabled/offscreen |
| `ui_wrong_window` | Scope window missing |
| `ui_login_pending` | Login UI still present (phone not confirmed) |
| `ui_sync_timeout` | Element/window wait timed out |
| `ui_scope_required` | Probe 缺少 `--hwnd` / `--title` |
| `ui_desktop_not_found` | 桌面 Shell 窗口未找到 |

Audit records (`audit.jsonl`) include `ui_phase` and `error_code` when applicable.

## Process launch (GUI apps)

Use `shell.run` with:

```yaml
params:
  command: "C:\\Path\\App.exe"
  wait: detach
  if_running: skip
  if_running_window:
    title_contains: "MyApp"
    prefer_largest: true
```

## Directive inputs

Optional inputs with `default` in YAML are applied when the caller omits the key **or**
passes an empty string. Sync `%AppData%\corex\data\directives\` with
`examples/directives/` after upgrades.

## Smoke test

[`examples/directives/ui-smoke-notepad.yaml`](../examples/directives/ui-smoke-notepad.yaml)
exercises all 13 `ui.*` actions against **Win11 Notepad** (Simplified Chinese).

Win11 Notepad UI notes:

- Tab label shows **无标题**; top-level HWND title is usually **无标题 - Notepad**
- Default `window_hint` is **无标题** (not `Notepad`)
- Menu bar: **文件 / 编辑 / 查看**; edit area is UIA `Document`
- Rich-text toolbar buttons are fragile across builds — prefer `ui.element.click` on `Document` + `ui.type`

```powershell
corex run ui-smoke-notepad
corex run ui-smoke-notepad -i close_after=false   # leave window open for inspection
```

Rebuild corex after pulling (needs `End`/`Home` in `ui.key`).

Sync `%AppData%\corex\data\directives\` after upgrades.

## Runtime UI profile

Configure in `config/corex.toml` or `<data_dir>/config.toml`:

```toml
[runtime]
ui_profile = "patient"          # baseline | fast | patient
# ui_max_selector_chain = 12    # optional override
# ui_max_settle_ms = 0          # cap total ui.wait ms (fast preset = 2000)
```

| Profile | `ui_max_selector_chain` | `ui_max_settle_ms` |
|---------|-------------------------|--------------------|
| `baseline` | 8 | 0 (unlimited) |
| `fast` | 5 | 2000 |
| `patient` | 12 | 0 |

Legacy `ui_profile = "default"` is accepted as an alias for `baseline`.

Win11 Notepad: tab title changes after typing (无标题 → first line). Window steps at the
start use `window_hint`; post-close checks should use `hwnd: '{{target_window.hwnd}}'`, not title.

## Related

- [actions.md](./actions.md)
- [compliance.md](./compliance.md)
- [enterprise-deploy.md](./enterprise-deploy.md)
- [examples/directives/wechat-send-message.yaml](../examples/directives/wechat-send-message.yaml)
- [examples/directives/ui-smoke-notepad.yaml](../examples/directives/ui-smoke-notepad.yaml)
