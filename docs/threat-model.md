# Threat model (corex v5 builtins)

## Trust boundaries

| Component | Trust level | Notes |
|-----------|-------------|-------|
| Directive YAML author | Semi-trusted | Can declare steps + permissions |
| corex-daemon | Trusted | Holds IPC token, runs actions |
| Builtin actions | Trusted code | Feature-gated at compile time |
| External HTTP targets | Untrusted | `http.request` |
| Local filesystem | Sensitive | `file.write` patch modes |

## High-risk actions

| Action | Risk | Mitigation |
|--------|------|------------|
| `shell.run` / `exec.run` | Arbitrary process / scripts | `permissions.shell`; per-Action `disabled_actions`; prefer explicit `host` for audit; `filesystem_roots` confines `exec.run` **script** and both Actions' **cwd** (`shell.run` **command** is not path-confined — PATH lookup) |
| `ui.*` | Input injection, app control | `permissions.ui`; enterprise default off |
| `capture.screenshot` / `capture.ocr` / `capture.monitors` | PII / secrets on screen; monitor layout | `permissions.capture`; enterprise preset disables; no OCR text in history |
| `keyring.*` / `scan.os` | Secrets / host inventory | `permissions.secret` (keyring); enterprise `disabled_actions` |
| `file.write` (`regex` mode) | ReDoS | Pattern length + output byte limits |
| `clipboard.set` (`image`) | Data exfil via clip | `permissions.clipboard` |
| `corex ui` probe CLI | Enumerate windows/elements; PII in stdout | Same `plugins.disabled` / `disabled_actions` / `strict_permissions` as daemon; audit as `ui.probe`; `--redact` |

## Removed / migrated

- **`capture.clipboard`** removed — use `clipboard.set` with `format: image` or `text`.
- **`ui.find` / `ui.wait`** removed — use `ui.window.find` / `ui.window.wait`.

## Data flows

```
Directive → Pipeline → ActionStore → builtin / WASM
                ↓
         history.jsonl (directive-level)
         audit.jsonl (step-level, redacted)
```

## Redaction policy (implemented)

| Data | Logged? |
|------|---------|
| `action_id`, `step_id`, `duration_ms`, `ok` | Yes (`audit.jsonl`) |
| Directive history `error` | Short class + truncated / path-redacted text (`history.jsonl`) |
| HTTP body, OCR text, clipboard payload | **No** |
| Sensitive action params | **Not logged** (no values, no key dumps) |

## Compliance notes

- UI automation examples (e.g. WeChat) are **experimental**; see [compliance.md](./compliance.md).
- Enable `strict_permissions` + `filesystem_roots` in enterprise deployments.
- Local `corex run` bypasses daemon IPC; see [enterprise-deploy.md](./enterprise-deploy.md#cli-trust-boundary).
