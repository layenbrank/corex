# Builtin Actions (v5)

> **文档导航：** [文档中心](./README.md) · [指令与输入配置（中文）](./guide/指令与输入配置.md)

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
| `http.send` | `act-http` | `url`; `method?` (GET), `params?`/`query?`, `headers?`, `token?`, `auth?`, `body?`, `json?`, `form?`, `timeout_ms?`, `follow_redirects?` | HTTP client（curl/fetch 风格） |
| `clipboard.get` | `act-clipboard` | `format?` (`text` \| `image`) | Read clipboard |
| `clipboard.set` | `act-clipboard` | `format?`; `text?`; `file?` (image) | Write clipboard |
| `notify.send` | `act-notify` | `summary`; `body?`, `appname?` (corex) | Desktop notification |
| `file.read` | `act-file` | `path` | Read file → string |
| `file.write` | `act-file` | `path`; `content?`; `mode?` (`overwrite` \| `replace_between` \| `regex` \| `json_set`); `backup?` | Write or patch file |
| `file.copy` | `act-file` | `from`, `to` | Copy file |
| `file.delete` | `act-file` | `path` | Delete file |
| `template.render` | `act-template` | `template`; `context?` (map) | MiniJinja render |
| `cron.schedule` | `act-cron` | `expr`; `directive?` | Register cron job on active `corex cron` supervisor |
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

> **Runnable examples:** multi-step flows in [`examples/directives/`](../examples/directives/README.md); single-action stubs in [`examples/actions/`](../examples/actions/README.md).

## Per-action examples

Each entry includes a minimal Directive step, an IPC invoke line, and a link to a runnable file. Platform tags: **Win** = Windows-only, **pdfium** = needs native pdfium, **cron sup** = needs `corex cron run`.

### System & shell

#### `shell.run`

- Example: [`examples/actions/shell.run.yaml`](../examples/actions/shell.run.yaml) · [`shell-host-demo.yaml`](../examples/directives/shell-host-demo.yaml)

```yaml
- id: run
  action: shell.run
  params:
    command: echo
    args: ["hello"]
    host: none
    wait: sync
  save_to: out
```

IPC: `{"type":"invoke","action":"shell.run","params":{"command":"echo","args":["hello"]}}`

#### `exec.run`

- Example: [`examples/actions/exec.run.yaml`](../examples/actions/exec.run.yaml) · [`exec-run-demo.yaml`](../examples/directives/exec-run-demo.yaml)

```yaml
- id: seed
  action: file.write
  params:
    path: "{{env.TEMP}}/demo.ps1"
    content: "Write-Output hello"
    mode: overwrite
    create_dirs: true
- id: run
  action: exec.run
  params:
    script: "{{env.TEMP}}/demo.ps1"
    host: auto
  save_to: out
```

IPC: `{"type":"invoke","action":"exec.run","params":{"script":"C:/Temp/demo.ps1","host":"auto"}}`

#### `bootstrap.env` / `bootstrap.inspect` / `bootstrap.force` (**Win**)

- Example: [`examples/actions/bootstrap.inspect.yaml`](../examples/actions/bootstrap.inspect.yaml) · [`bootstrap-demo.yaml`](../examples/directives/bootstrap-demo.yaml)

```yaml
- id: inspect
  action: bootstrap.inspect
  params: {}
  save_to: out
```

IPC: `{"type":"invoke","action":"bootstrap.inspect","params":{}}`

#### `scan.os`

- Example: [`examples/actions/scan.os.yaml`](../examples/actions/scan.os.yaml) · [`scan-env-demo.yaml`](../examples/directives/scan-env-demo.yaml)

```yaml
- id: scan
  action: scan.os
  params: {}
  save_to: info
```

IPC: `{"type":"invoke","action":"scan.os","params":{}}`

#### `cron.schedule` (**cron sup**)

- Example: [`examples/actions/cron.schedule.yaml`](../examples/actions/cron.schedule.yaml) · [`cron-schedule-demo.yaml`](../examples/directives/cron-schedule-demo.yaml)

```yaml
- id: reg
  action: cron.schedule
  params:
    expr: "0 0 12 * * *"
    directive: hello
  save_to: job
```

IPC: `{"type":"invoke","action":"cron.schedule","params":{"expr":"0 0 12 * * *","directive":"hello"}}`

### Network & templates

#### `http.send`

- Example: [`examples/actions/http.send.yaml`](../examples/actions/http.send.yaml) · [`http-post-json.yaml`](../examples/directives/http-post-json.yaml)

```yaml
- id: get
  action: http.send
  params:
    url: "https://httpbin.org/get"
    method: GET
    timeout_ms: 15000
  save_to: resp
```

IPC: `{"type":"invoke","action":"http.send","params":{"url":"https://httpbin.org/get"}}`

#### `template.render`

- Example: [`examples/actions/template.render.yaml`](../examples/actions/template.render.yaml) · [`hello.yaml`](../examples/directives/hello.yaml)

```yaml
- id: tpl
  action: template.render
  params:
    template: "Hello, {{ name }}!"
    context: { name: "{{input.who}}" }
  save_to: message
```

IPC: `{"type":"invoke","action":"template.render","params":{"template":"Hi","context":{"name":"x"}}}`

### Filesystem

#### `file.read` / `file.write` / `file.copy` / `file.delete`

- Examples: [`file.write.yaml`](../examples/actions/file.write.yaml) · [`file.copy.yaml`](../examples/actions/file.copy.yaml) · [`file-ops-demo.yaml`](../examples/directives/file-ops-demo.yaml) · [`file-write-modes.yaml`](../examples/directives/file-write-modes.yaml)

```yaml
- id: write
  action: file.write
  params:
    path: "{{env.TEMP}}/out.txt"
    content: "hello"
    mode: overwrite
    create_dirs: true
```

```yaml
- id: copy
  action: file.copy
  params:
    from: "./examples/directives/hello.yaml"
    to: "{{env.TEMP}}/hello-copy.yaml"
```

IPC: `{"type":"invoke","action":"file.copy","params":{"from":"a.txt","to":"b.txt"}}`

#### `copy.run`

- Example: [`examples/actions/copy.run.yaml`](../examples/actions/copy.run.yaml) · [`copy-demo.yaml`](../examples/directives/copy-demo.yaml)

```yaml
- id: copy
  action: copy.run
  params:
    from: "./examples/directives"
    to: "{{env.TEMP}}/copy-out"
    empty: false
  save_to: result
```

#### `scrub.run`

- Example: [`examples/actions/scrub.run.yaml`](../examples/actions/scrub.run.yaml) · [`scrub-demo.yaml`](../examples/directives/scrub-demo.yaml)

```yaml
- id: scrub
  action: scrub.run
  params:
    source: "{{env.TEMP}}/work"
    target: "stale.tmp"
    recursive: true
```

#### `shade.convert`

- Example: [`examples/actions/shade.convert.yaml`](../examples/actions/shade.convert.yaml) · [`shade-demo.yaml`](../examples/directives/shade-demo.yaml)

```yaml
- id: convert
  action: shade.convert
  params:
    from: "{{env.TEMP}}/in.png"
    to: "{{env.TEMP}}/out.jpg"
    format: jpeg
    quality: 85
```

#### `compression.compress` / `compression.decompress`

- Example: [`examples/actions/compression.compress.yaml`](../examples/actions/compression.compress.yaml) · [`compression-demo.yaml`](../examples/directives/compression-demo.yaml)

```yaml
- id: zip
  action: compression.compress
  params:
    from: "./examples/directives/hello.yaml"
    to: "{{env.TEMP}}/demo.zip"
    format: zip
    level: 6
```

IPC: `{"type":"invoke","action":"compression.compress","params":{"from":"dir","to":"out.zip","format":"zip"}}`

### Generate & codec

#### `generate.uuid` / `generate.cvid` / `generate.timestamp`

- Example: [`examples/actions/generate.uuid.yaml`](../examples/actions/generate.uuid.yaml) · [`generate-demo.yaml`](../examples/directives/generate-demo.yaml)

```yaml
- id: uuids
  action: generate.uuid
  params: { count: 2, uppercase: false }
  save_to: ids
```

#### `generate.path`

- Example: [`examples/actions/generate.path.yaml`](../examples/actions/generate.path.yaml) · [`generate-path-demo.yaml`](../examples/directives/generate-path-demo.yaml)

```yaml
- id: paths
  action: generate.path
  params:
    from: "./examples/directives"
    to: "{{env.TEMP}}/paths.txt"
    transform: "{{path}}"
    includes: ["*.yaml"]
```

#### `codec.base64.encode` / `decode` / `codec.hash.md5` / `codec.json.parse`

- Example: [`examples/actions/codec.base64.encode.yaml`](../examples/actions/codec.base64.encode.yaml) · [`codec-pipeline.yaml`](../examples/directives/codec-pipeline.yaml)

```yaml
- id: b64
  action: codec.base64.encode
  params: { input: "corex" }
  save_to: encoded
```

IPC: `{"type":"invoke","action":"codec.json.parse","params":{"text":"{\"a\":1}"}}`

### Desktop & secrets

#### `clipboard.get` / `clipboard.set`

- Example: [`examples/actions/clipboard.set.yaml`](../examples/actions/clipboard.set.yaml) · [`clipboard-notify.yaml`](../examples/directives/clipboard-notify.yaml)

```yaml
- id: clip
  action: clipboard.set
  params: { format: text, text: "hello" }
```

#### `notify.send`

- Example: [`examples/actions/notify.send.yaml`](../examples/actions/notify.send.yaml)

```yaml
- id: toast
  action: notify.send
  params: { summary: "Corex", body: "done" }
```

#### `keyring.get` / `keyring.set`

- Example: [`examples/actions/keyring.set.yaml`](../examples/actions/keyring.set.yaml) · [`keyring-demo.yaml`](../examples/directives/keyring-demo.yaml)

```yaml
- id: store
  action: keyring.set
  params:
    service: my-app
    user: demo
    password: "CHANGE-ME"
```

Requires `permissions.secret: true` when permissions are declared.

### Capture (**Win** for screenshot/monitors/ocr)

#### `capture.screenshot` / `capture.crop` / `capture.monitors` / `capture.ocr`

- Example: [`examples/actions/capture.screenshot.yaml`](../examples/actions/capture.screenshot.yaml) · [`capture-demo.yaml`](../examples/directives/capture-demo.yaml)

```yaml
- id: shot
  action: capture.screenshot
  params:
    to: "{{env.TEMP}}/shot.png"
    format: png
  save_to: path
```

IPC: `{"type":"invoke","action":"capture.screenshot","params":{"to":"C:/Temp/shot.png"}}`

### UI automation (**Win**)

All `ui.*` actions: [`ui-smoke-notepad.yaml`](../examples/directives/ui-smoke-notepad.yaml) (13 actions) · stub: [`ui.window.list.yaml`](../examples/actions/ui.window.list.yaml)

```yaml
- id: wins
  action: ui.window.list
  params: {}
  save_to: windows
```

```yaml
- id: click
  action: ui.element.click
  params:
    name_contains: "编辑"
    safe: true
  save_to: clicked
```

Requires `permissions.ui: true` when permissions are declared.

### PDF / morph

#### `morph.export` / `morph.merge` / `morph.split`

- Example: [`examples/actions/morph.export.yaml`](../examples/actions/morph.export.yaml) · [`morph-demo.yaml`](../examples/directives/morph-demo.yaml)

```yaml
- id: export
  action: morph.export
  params:
    src: "{{input.pdf_path}}"
    dest: "{{env.TEMP}}/copy.pdf"
```

#### `morph.meta` / `morph.render` (**pdfium**)

- Example: [`examples/actions/morph.meta.yaml`](../examples/actions/morph.meta.yaml)

```yaml
- id: meta
  action: morph.meta
  params: { path: "{{input.pdf_path}}" }
```

These return an error when pdfium is not bundled in the current build.

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

1. **`cron.schedule`** — Requires an active `corex cron run` supervisor; registers jobs on the shared `CronEngine`.
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
