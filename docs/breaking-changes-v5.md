# Breaking Changes — Corex v5.0.0

v5 is a **breaking** rename and enterprise hardening release on top of the v4 action runtime.
There is **no** v4 IPC/YAML compatibility shim.

## Shortcut → Directive

| Area | v4 | v5 |
|------|----|----|
| Domain type | `Shortcut` | `Directive` |
| Data dir | `<data>/shortcuts` | `<data>/directives` |
| Examples | `examples/shortcuts/` | `examples/directives/` |
| Docs | `docs/shortcut-yaml.md` | `docs/directive-yaml.md` |
| IPC | `run_shortcut` / `list_shortcuts` | `run_directive` / `list_directives` |
| History JSONL field | `shortcut` | `directive` |
| CLI help | “shortcut” | “directive” / 指令 |

Migrate YAML files by moving them into the new directory; document schema (`name`, `steps`, `permissions`) is unchanged.

## UI action renames

| Removed | Replacement |
|---------|-------------|
| `ui.find` | `ui.window.find` (top-level HWND / title) |
| `ui.wait` | `ui.window.wait` |

New element APIs (Windows UIAutomation):

- `ui.element.list` / `ui.element.find` / `ui.element.click` / `ui.element.wait`

Use `ui.window.*` for top-level windows; use `ui.element.*` for in-app controls (`name`, `automation_id`, `control_type`).

## Enterprise controls

- `[runtime].strict_permissions` — deny unrestricted directives; also blocks `corex ui` probe (ui.*) and daemon Invoke of permissioned actions
- `[runtime].filesystem_roots` — confine `file.*` via `confine_under`
- `corex validate --strict` — require declared permissions covering all steps
- Step audit: `<data>/audit.jsonl` (`action_id`, `duration_ms`, redacted; no OCR/HTTP body)
- Preset: [`config/enterprise.toml`](../config/enterprise.toml)
- Compliance: [`docs/compliance.md`](./compliance.md)

## Interactive UI CLI (`corex ui`)

Nested commands (no aliases for removed flat names):

| Removed | Replacement |
|---------|-------------|
| `corex ui windows` / `ui list` | `corex ui window list` |
| (desktop icons via silent list fallback) | `corex ui window desktop` |
| `corex ui find` / `ui at` | `corex ui element get` / `element point` |
| pick under old path | `corex ui element pick` |

Probe gate/audit ids: `ui.window.list`, `ui.window.desktop`, `ui.element.list`, `ui.element.find`, `ui.element.point`, `ui.element.pick`.
Also honors `[plugins].disabled = ["ui"]`.

## Process launch (`shell.run` / `exec.run`)

Shared launch kernel; both Actions remain. Parameter cleanup:

| Removed | Replacement |
|---------|-------------|
| `shell.run` param `shell: true` | `host: cmd` (Windows) or omit (`auto` → direct exec for commands) |
| `exec.run` param `capture` (`json` / `text` / `none`) | Always returns `{ stdout, stderr, exit_code, success }` |
| `exec.run` JSON/`path` stdout protocol | Compose with `codec.json.parse` / `{{step.*.stdout}}` |

New optional param on both: `host` (`none` \| `cmd` \| `powershell` \| `pwsh` \| `auto`).

## Version

Workspace package version is **5.0.0**.

## Related

- [breaking-changes-v4.md](./breaking-changes-v4.md)
- [directive-yaml.md](./directive-yaml.md)
- [threat-model.md](./threat-model.md)
- [enterprise-deploy.md](./enterprise-deploy.md)
