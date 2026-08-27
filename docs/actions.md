# Builtin Actions (v5)

Action IDs registered by `corex-registry` builtins (`crates/registry/src/builtin/`). Feature gates are `act-*` (enabled via `full` in default daemon/CLI builds).

On Windows, `windows` crate features are enabled **per gate** (not globally):

| Bundle / gate | Pulls |
|---------------|--------|
| `win32-base` | Win32 messaging (`act-ui` / process helpers) |
| `win32-process` | + ToolHelp (`act-shell` / `act-exec`) |
| `winrt-ocr` | WinRT Imaging/OCR/Storage (`act-capture`) |

`windows` is `optional` + `default-features = false` in the workspace; only enabled gates compile those APIs.

Invoke via Directive YAML (`action: <id>`) or IPC `{"type":"invoke","action":"<id>","params":{...}}`.

## Catalog

| Action ID | Feature | Required / common params | Notes |
|-----------|---------|--------------------------|--------|
| `shell.run` | `act-shell` | `command` (str); `args?`, `cwd?`, `host?`, `allow_nonzero?` | Process launcher (facade); always returns `{stdout,stderr,exit_code,success}` |
| `http.request` | `act-http` | `url`; `method?` (GET), `headers?`, `body?`, `json?` | HTTP client |
| `clipboard.get` | `act-clipboard` | `format?` (`text` \| `image`) | Read clipboard |
| `clipboard.set` | `act-clipboard` | `format?`; `text?`; `file?` (image) | Write clipboard |
| `notify.send` | `act-notify` | `summary`; `body?`, `appname?` (corex) | Desktop notification |
| `file.read` | `act-file` | `path` | Read file → string |
| `file.write` | `act-file` | `path`; `content?`; `mode?` (`overwrite` \| `replace_between` \| `regex` \| `json_set`); `backup?` | Write or patch file |
| `file.copy` | `act-file` | `from`, `to` | Copy file |
| `file.delete` | `act-file` | `path` | Delete file |
| `template.render` | `act-template` | `template`; `context?` (map) | MiniJinja render |
| `cron.schedule` | `act-cron` | `expr`; `Directive?` | **Not implemented** — `execute` returns an error asking for an external scheduler |
| `keyring.get` | `act-keyring` | `service`, `user` | OS keyring read |
| `keyring.set` | `act-keyring` | `service`, `user`, `password` | OS keyring write |
| `copy.run` | `act-copy` | `from`, `to`; `empty?`, `includes?`, `excludes?` | Tree / filtered copy |
| `scrub.run` | `act-scrub` | `source`, `target`; `recursive?` | Path scrub / sanitize |
| `shade.convert` | `act-shade` | `from`, `to`; `format?`, `quality?` | Image convert |
| `compression.compress` | `act-compression` | `from`, `to`; `format?` (zip), `level?`, `includes?`, `excludes?` | zip / tar.gz; **`7z` soft-fails** (error: not enabled in this build) |
| `compression.decompress` | `act-compression` | `from`, `to`; `format?` | zip / tar.gz; **`7z` soft-fails** similarly |
| `generate.uuid` | `act-generate` | `count?`, `uppercase?` | UUID(s) |
| `generate.cvid` | `act-generate` | — | Compact id |
| `generate.timestamp` | `act-generate` | `format?`, `utc?` | Current time `{ value, unix, iso8601 }` |
| `generate.path` | `act-generate` | `from`, `to`, `transform`; … | Path transform / rename helpers |
| `exec.run` | `act-exec` | `script` (path); `args?`, `cwd?`, `host?`, `allow_nonzero?` | Script-file runner (same launch kernel as `shell.run`) |
| `bootstrap.env` | `act-bootstrap` | — | Windows-oriented env bootstrap (errors off Windows) |
| `bootstrap.inspect` | `act-bootstrap` | — | Inspect bootstrap state |
| `bootstrap.force` | `act-bootstrap` | — | Force bootstrap (Windows) |
| `codec.base64.encode` | `act-codec` | `input?` / `file?`; `output?` | Base64 encode |
| `codec.base64.decode` | `act-codec` | `input?` / `file?`; `output?` | Base64 decode |
| `codec.hash.md5` | `act-codec` | `input?` / `file?`; `output?` | MD5 digest |
| `codec.json.parse` | `act-codec` | `text` | Parse JSON string → structured `Value` |
| `scan.os` | `act-scan` | — | OS / environment scan |
| `capture.screenshot` | `act-capture` | `to`; `format?` (png), `quality?` | Screenshot (Windows backend) |
| `capture.clipboard` | — | — | **Removed** — use `clipboard.set` with `format: image` |
| `capture.ocr` | `act-capture` | `file`; `language?` | OCR (Windows Media OCR) |
| `capture.crop` | `act-capture` | `from`, `to`, `x`, `y`, `width`, `height` | Crop image |
| `capture.monitors` | `act-capture` | — | List monitors (Windows backend) |
| `ui.window.list` | `act-ui` | — | List top-level windows (`hwnd`/`title`/`class`/`pid`) |
| `ui.window.focus` | `act-ui` | `title_contains?`, `hwnd?`, `prefer_largest?`, `class_name?` | Focus window; updates ui_session scope |
| `ui.window.find` | `act-ui` | same as focus | Find top-level window |
| `ui.window.wait` | `act-ui` | `title_contains?`, `timeout_ms`, `prefer_largest?` | Wait for window |
| `ui.element.list` | `act-ui` | `hwnd?`, `title_contains?`, `depth?`, `limit?` | List child elements (UIA) |
| `ui.element.find` | `act-ui` | `name?`, `name_contains?`, `automation_id?`, `control_type?`, `selectors?` | Find in-app element |
| `ui.element.exists` | `act-ui` | same as find | Probe `{ found, element? }` |
| `ui.element.click` | `act-ui` | same as find + `safe?` (default true) | Click element (waits enabled when safe) |
| `ui.element.wait` | `act-ui` | same as find + `state?`, `timeout_ms`, `poll_interval_ms?` | Wait `present` / `absent` / `enabled` |
| `ui.wait` | `act-ui` | `ms` | Fixed sleep (fallback; capped by `ui_max_settle_ms`) |
| `ui.click` | `act-ui` | `x`, `y` | Click at screen coordinates |
| `ui.type` | `act-ui` | `text` | Type text |
| `ui.key` | `act-ui` | `keys` | Key combo (`Enter`, `Ctrl+F`, …) |
| `morph.meta` | `act-morph` | `path` | PDF meta (needs pdfium when enabled) |
| `morph.render` | `act-morph` | `path`; `offset?`, `scale?` | PDF page render |
| `morph.export` | `act-morph` | `src`, `dest` | PDF export |
| `morph.merge` | `act-morph` | `paths`, `dest` | Merge PDFs |
| `morph.split` | `act-morph` | `path`, `dir`; `limit?`, `ranges?` | Split PDF |

## Process launch (`shell.run` / `exec.run`)

Both facades share one **process launch kernel** (`process_launch`). They differ only in product intent:

| Facade | Required param | Target kind | Typical use |
|--------|----------------|-------------|-------------|
| `shell.run` | `command` | command / binary | `npm`, `fnm`, absolute `.exe` |
| `exec.run` | `script` | script file (must exist) | `.bat` / `.ps1` / `.sh` path you choose |

**`host`** (optional, default `auto`):

| Value | Behavior |
|-------|----------|
| `none` | Direct `Command::new(program)` + args |
| `cmd` | Windows `cmd /C` …; Unix `sh -c` / script path |
| `powershell` | Windows PowerShell 5.x `-File` / `-Command` |
| `pwsh` | PowerShell 7+ |
| `auto` | Command → `none`; script by extension (`.ps1`→pwsh/powershell, `.bat`/`.cmd`→cmd, `.sh`→sh) |

**GUI / single-instance** (optional):

| Param | Values | Default |
|-------|--------|---------|
| `wait` | `sync` \| `detach` | `sync` |
| `if_running` | `launch` \| `skip` \| `fail` | `launch` |
| `if_running_window` | `{ title_contains, title_excludes?, prefer_largest? }` | — |

Returns include `detached`, `skipped`, `reason`, `pid` when applicable.

See [ui-automation.md](./ui-automation.md).

Enterprise: set an explicit `host` for auditability; disable either Action via `disabled_actions` while keeping `permissions.shell`.

## Notable caveats

1. **`cron.schedule`** — Registered for schema/discovery completeness; calling it **errors** (`尚未实现`). Schedule externally (`systemd`, Task Scheduler, or call `corex run`).
2. **`compression.*` + `7z`** — Format is recognized but **not enabled** in the current build; use `zip` or `tar.gz`.
3. **Platform Actions** — `capture.*` (screenshot/monitors), `bootstrap.*` (env/force), and some `morph.*` paths may error when native backends / pdfium are unavailable.
4. **Runtime disable** — `[plugins].disabled` / `disabled_actions` in config can hide Actions after registration.
5. **Permissions** — When a Directive declares any permission flag, undeclared categories are denied (see [directive-yaml.md](./directive-yaml.md)).

## WASM plugins

Third-party `*.wasm` components may register additional IDs via discovery. Host bindgen is incomplete — failed loads are skipped. See [plugins/README.md](../plugins/README.md).

## Related

- [directive-yaml.md](./directive-yaml.md)
- [ui-automation.md](./ui-automation.md)
- [ipc-protocol.md](./ipc-protocol.md)
- [architecture.md](./architecture.md)
