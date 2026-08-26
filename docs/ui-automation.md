# UI automation (corex builtins)

Enterprise guide for `ui.*` actions on Windows (UIAutomation + Win32). macOS/Linux
backends are planned; see [cross-platform-backends.md](./cross-platform-backends.md).

## Principles

1. **Element sync over fixed sleep** — prefer `ui.element.wait` (`present` / `absent` /
   `enabled`) over `ui.wait`. Use `ui.wait` only as a last resort; enterprise config may
   cap total settle time via `[runtime].ui_max_settle_ms`.
2. **Multi-attribute selectors** — combine `name`, `automation_id`, `control_type`; use
   `selectors: [...]` fallback chains (max 5) when apps change between versions.
3. **Session scope** — after `ui.window.find|wait|focus`, `ExecutionContext.ui_session`
   caches `scope_hwnd`; later steps may omit `title_contains`.
4. **Human checkpoints** — flows that require phone approval (e.g. WeChat 4.x「进入微信」)
   must use `ui.element.wait` with `state: absent` and a generous `timeout_ms`; failures
   return `[ui_login_pending]`.

## Auto.js / AutoX mapping

| Auto.js | corex |
|---------|-------|
| `selector().findOne(timeout)` | `ui.element.find` / `ui.element.exists` |
| `exists()` | `ui.element.exists` → `{ found: bool }` |
| `waitFor()` | `ui.element.wait` `state: present` |
| `clickable()` + click | `ui.element.click` with `safe: true` (default) |
| `sleep()` | **avoid** — use `ui.element.wait`; fallback `ui.wait` |

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
passes an empty string. Sync `%LOCALAPPDATA%\\corex\\directives\\` with
`examples/directives/` after upgrades.

## Related

- [actions.md](./actions.md)
- [compliance.md](./compliance.md)
- [enterprise-deploy.md](./enterprise-deploy.md)
- [examples/directives/wechat-send-message.yaml](../examples/directives/wechat-send-message.yaml)
