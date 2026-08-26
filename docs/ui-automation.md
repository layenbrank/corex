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
| `selector().findOne(timeout)` | `ui.element.find` | `corex ui find --name "…"` |
| `exists()` | `ui.element.exists` | (use `find` + check JSON) |
| `waitFor()` | `ui.element.wait` `state: present` | — |
| `clickable()` + click | `ui.element.click` `safe: true` | JSON 字段 `clickable` |
| `bounds()` | — | JSON 字段 `bounds: {x,y,width,height}` |
| `id()` / `className()` | `automation_id` / `class` | `find` / `pick` 输出 |
| 布局分析 / 控件树 | `ui.element.list` | `corex ui list` |
| 点击选控件（Inspect） | — | `corex ui pick` |
| `sleep()` | **avoid** — `ui.element.wait`; fallback `ui.wait` | — |

## Interactive probe (`corex ui`)

Windows-only CLI for exploring selectors before writing directives (对标 Auto.js 布局分析 / FlaUInspect hover pick).

```powershell
# 枚举顶层窗口
corex ui windows

# 列出窗口内 UIA 子树（先打开记事本）
corex ui list --title "无标题" --depth 3

# 按 selector 查找（输出含 bounds / enabled / clickable / selectors_yaml）
corex ui find --title "无标题" --control-type document

# 坐标 hit-test（无 overlay）
corex ui at --x 640 --y 480

# 浏览器式点击选择：悬停高亮，左键确认，Esc 取消
corex ui pick
corex ui pick --scope-hwnd 0x00123456 --copy-yaml   # 限定在某个窗口内
```

`pick` / `at` / `find` 输出 JSON，包含：

- `bounds`, `enabled`, `clickable`, `control_type`（人类可读小写）
- `selectors` — 建议 fallback 链（AutomationId → name+type → name）
- `selectors_yaml` — 可直接粘贴进 directive step 的 YAML 片段

Workflow（编写新 directive 时）：

1. 打开目标应用
2. `corex ui windows` → 记下 `hwnd` 或 `title`
3. `corex ui pick`（或 `pick --scope-hwnd …`）点击目标控件
4. 复制输出中的 `selectors_yaml` 到 YAML step
5. `corex ui find …` 验证 selector 能命中

## Structured errors

| Code | Meaning |
|------|---------|
| `ui_selector_not_found` | No element matched selector chain |
| `ui_not_clickable` | Element found but disabled/offscreen |
| `ui_wrong_window` | Scope window missing |
| `ui_login_pending` | Login UI still present (phone not confirmed) |
| `ui_sync_timeout` | Element/window wait timed out |

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
