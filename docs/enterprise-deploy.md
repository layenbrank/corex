# Enterprise deployment

Recommended settings for locked-down environments.
Prefer copying [`config/enterprise.toml`](../config/enterprise.toml) as a starting point.

## Config (`config/corex.toml` or override)

```toml
[runtime]
strict_permissions = true
filesystem_roots = ["C:/ProgramData/corex/data"]

[plugins]
disabled_actions = [
  "shell.run",
  "exec.run",
  "ui.window.list",
  "ui.window.focus",
  "ui.window.find",
  "ui.window.wait",
  "ui.element.list",
  "ui.element.find",
  "ui.element.click",
  "ui.element.wait",
  "ui.element.exists",
  "ui.wait",
  "ui.click",
  "ui.type",
  "ui.key",
  "capture.ocr",
  "capture.screenshot",
]

[logging]
json = true
```

`strict_permissions` rejects directives with no permission flags (unrestricted).  
`corex validate --strict` additionally checks that declared flags cover every step’s action kind.

Enable high-privilege actions only when required, by removing IDs from `disabled_actions` and declaring matching `permissions` in each directive.

Validate before deploy:

```bash
corex validate --strict path/to/directive.yaml
```

## Directive permissions

Always declare minimal flags:

```yaml
permissions:
  network: true    # http.request
  filesystem: true # file.write, file.read, codec.*, copy.*, …
  shell: true      # shell.run
  clipboard: true  # clipboard.get/set
  ui: true         # ui.*
  capture: true    # capture.screenshot / capture.ocr / capture.monitors
  secret: true     # keyring.*
```

## IPC

- Set `COREX_TOKEN` or `[daemon].token`; restrict file mode on `<data-dir>/token` (Unix `0600`).
- Do not expose the daemon socket/pipe to untrusted users.

## Logging & audit

- Set `[logging].json = true` for SIEM ingestion.
- Step audit is written to `<data-dir>/audit.jsonl` (`action_id`, `duration_ms`, `ok`).
- Logs must not include HTTP bodies or OCR full text (engine redaction).

## Related

- [compliance.md](./compliance.md)
- [threat-model.md](./threat-model.md)
- [directive-yaml.md](./directive-yaml.md)
- [breaking-changes-v5.md](./breaking-changes-v5.md)
