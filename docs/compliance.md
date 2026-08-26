# Compliance (corex directives)

## Scope

This document covers **authorized desktop automation** using corex directives
(`ui.*`, `capture.*`, `shell.run`). It is not legal advice.

## Principles

1. **Authorization first** — Only automate apps and accounts the operator is
   allowed to control. Do not use corex for covert employee monitoring without
   a written policy and legal review.
2. **Least privilege** — Enable `strict_permissions`, set `filesystem_roots`,
   and list high-risk IDs in `disabled_actions` (see [enterprise-deploy.md](./enterprise-deploy.md)).
3. **No silent production sync** — Example directives under `examples/`
   (including WeChat / IM recipes) must **not** be copied into the daemon
   directives directory by default installers or enterprise MDM packages.
4. **Third-party ToS** — Automating messaging apps (WeChat, Slack, etc.) may
   violate the vendor Terms of Service. Treat those recipes as **experimental**
   and fragile; obtain approval before any production use.
5. **PII / screen data** — Screenshot and OCR can capture secrets. Keep OCR
   text out of history/audit payloads (engine redaction). Restrict who can
   enable `permissions.capture` and `permissions.ui`.

## Recommended controls

| Control | Setting |
|---------|---------|
| Deny unrestricted (runtime) | `[runtime].strict_permissions = true` |
| Path confine | `[runtime].filesystem_roots = [...]` |
| Disable UI/OCR/shell by default | `[plugins].disabled_actions` (enterprise.toml) |
| Validate coverage (static) | `corex validate --strict path.yaml` |
| SIEM-friendly logs | `[logging].json = true` + `audit.jsonl` |

`strict_permissions` only rejects directives with **no** permission flags.  
`validate --strict` also walks every step and requires declared kinds to cover each action.

## Related

- [threat-model.md](./threat-model.md)
- [enterprise-deploy.md](./enterprise-deploy.md)
- [cross-platform-backends.md](./cross-platform-backends.md)
- [ui-automation.md](./ui-automation.md)

## Human-in-the-loop checkpoints

Some UI automation recipes (for example WeChat 4.x login in
[`examples/directives/wechat-send-message.yaml`](../examples/directives/wechat-send-message.yaml))
require **operator action on a second device** after the PC-side script clicks a button
(for example **进入微信**). corex does **not** automate the phone; it only waits for the
PC UI to change (`ui.element.wait` with `state: absent`).

| Checkpoint | Operator action | PC-side behavior | Failure signal |
|------------|-----------------|------------------|----------------|
| Mobile login confirm | Tap **同意/登录** on the phone | Wait up to `login_timeout_ms` | `[ui_login_pending]` in audit (`error_code`) |
| Abort / timeout | None | Pipeline fails closed; no partial send | Audit `ui_phase=login`, `selector_hint` |

Before enabling `permissions.ui` in production:

1. Document who may run login-dependent directives.
2. Set `login_timeout_ms` appropriately for your operators.
3. Sync directive YAML from `examples/` to `%LOCALAPPDATA%\corex\directives\` so
   `default` inputs and login branches match the signed recipe version.
4. Review `audit.jsonl` for `ui_login_pending` instead of generic execution errors.
