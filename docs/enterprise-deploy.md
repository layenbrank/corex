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
  "ui.window.desktop",
  "ui.window.focus",
  "ui.window.find",
  "ui.window.wait",
  "ui.element.list",
  "ui.element.find",
  "ui.element.point",
  "ui.element.pick",
  "ui.element.click",
  "ui.element.wait",
  "ui.element.exists",
  "ui.wait",
  "ui.click",
  "ui.type",
  "ui.key",
  "capture.ocr",
  "capture.screenshot",
  "capture.monitors",
  "keyring.get",
  "keyring.set",
  "scan.os",
]

[logging]
json = true
```

`strict_permissions` rejects directives with no permission flags (unrestricted).  
It also denies `corex ui` probe commands and daemon Invoke for any action that requires a permission kind (including all `ui.*`).  
`corex validate --strict` additionally checks that declared flags cover every step’s action kind.

When `filesystem_roots` is set, path I/O for `file.*` / `copy.*` / `codec.*` / … **and** `exec.run` script paths plus `shell.run`/`exec.run` `cwd` must resolve under those roots. `shell.run` `command` is not path-confined (PATH lookup).

Enable high-privilege actions only when required, by removing IDs from `disabled_actions` (and/or clearing `plugins.disabled`) and declaring matching `permissions` in each directive.

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
  shell: true      # shell.run / exec.run
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
- Directive-level `history.jsonl` stores short sanitized error classes (not full exception text).
- Logs must not include HTTP bodies or OCR full text (engine redaction).

## Minimal enterprise build

Default CLI/daemon crates enable the `full` feature set (all `act-*` + WASM). For a smaller attack surface, build with only the gates you need:

```bash
cargo build -p corex -p corex-daemon --release --no-default-features \
  --features act-file,act-template,act-http
```

On Windows, `act-ui` / `act-capture` / `act-shell` / `act-exec` pull in the corresponding `windows` API bundles (`win32-base`, `winrt-ocr`, `win32-process`). See [actions.md](./actions.md).

Prefer pairing a minimal build with `config/enterprise.toml` at runtime (`disabled_actions` + `strict_permissions`).

## CLI trust boundary

`corex run` executes directives **in-process** and does not go through daemon IPC. Any local user who can run the binary and read a non-strict config can invoke permissioned actions. Production deployments should:

1. Prefer `corex-daemon` + IPC token for untrusted clients.
2. Restrict filesystem ACLs on the `corex` / `corex-daemon` binaries and on `config/enterprise.toml` / data-dir.
3. Do not assume “daemon-only” security if operators also have shell access to `corex run`.

## Related

- [compliance.md](./compliance.md)
- [threat-model.md](./threat-model.md)
- [architecture.md](./architecture.md)
- [directive-yaml.md](./directive-yaml.md)
- [breaking-changes-v5.md](./breaking-changes-v5.md)
