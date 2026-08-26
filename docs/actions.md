# Builtin Actions (v4)

Action IDs registered by `corex-registry` builtins (`crates/registry/src/builtin/`). Feature gates are `act-*` (enabled via `full` in default daemon/CLI builds).

Invoke via Shortcut YAML (`action: <id>`) or IPC `{"type":"invoke","action":"<id>","params":{...}}`.

## Catalog

| Action ID | Feature | Required / common params | Notes |
|-----------|---------|--------------------------|--------|
| `shell.run` | `act-shell` | `command` (str); `args?`, `cwd?`, `shell?`, `allow_nonzero?` | Run a process / shell command |
| `http.request` | `act-http` | `url`; `method?` (GET), `headers?`, `body?`, `json?` | HTTP client |
| `clipboard.get` | `act-clipboard` | — | Read clipboard text |
| `clipboard.set` | `act-clipboard` | `text` | Write clipboard text |
| `notify.send` | `act-notify` | `summary`; `body?`, `appname?` (corex) | Desktop notification |
| `file.read` | `act-file` | `path` | Read file → string |
| `file.write` | `act-file` | `path`, `content`; `create_dirs?` (true) | Write file |
| `file.copy` | `act-file` | `from`, `to` | Copy file |
| `file.delete` | `act-file` | `path` | Delete file |
| `template.render` | `act-template` | `template`; `context?` (map) | MiniJinja render |
| `cron.schedule` | `act-cron` | `expr`; `shortcut?` | **Not implemented** — `execute` returns an error asking for an external scheduler |
| `keyring.get` | `act-keyring` | `service`, `user` | OS keyring read |
| `keyring.set` | `act-keyring` | `service`, `user`, `password` | OS keyring write |
| `copy.run` | `act-copy` | `from`, `to`; `empty?`, `includes?`, `excludes?` | Tree / filtered copy |
| `scrub.run` | `act-scrub` | `source`, `target`; `recursive?` | Path scrub / sanitize |
| `shade.convert` | `act-shade` | `from`, `to`; `format?`, `quality?` | Image convert |
| `compression.compress` | `act-compression` | `from`, `to`; `format?` (zip), `level?`, `includes?`, `excludes?` | zip / tar.gz; **`7z` soft-fails** (error: not enabled in this build) |
| `compression.decompress` | `act-compression` | `from`, `to`; `format?` | zip / tar.gz; **`7z` soft-fails** similarly |
| `generate.uuid` | `act-generate` | `count?`, `uppercase?` | UUID(s) |
| `generate.cvid` | `act-generate` | — | Compact id |
| `generate.path` | `act-generate` | `from`, `to`, `transform`; `index?`, `separator?`, `includes?`, `excludes?`, `uppercase?` | Path transform / rename helpers |
| `exec.run` | `act-exec` | `script`; `args?`, `cwd?`, `capture?` | Run a script file |
| `bootstrap.env` | `act-bootstrap` | — | Windows-oriented env bootstrap (errors off Windows) |
| `bootstrap.inspect` | `act-bootstrap` | — | Inspect bootstrap state |
| `bootstrap.force` | `act-bootstrap` | — | Force bootstrap (Windows) |
| `codec.base64.encode` | `act-codec` | `input?` / `file?`; `output?` | Base64 encode |
| `codec.base64.decode` | `act-codec` | `input?` / `file?`; `output?` | Base64 decode |
| `codec.hash.md5` | `act-codec` | `input?` / `file?`; `output?` | MD5 digest |
| `scan.os` | `act-scan` | — | OS / environment scan |
| `capture.screenshot` | `act-capture` | `to`; `format?` (png), `quality?` | Needs native screenshot backend |
| `capture.clipboard` | `act-capture` | `mode?`, `text?` | Clipboard capture helpers |
| `capture.crop` | `act-capture` | `from`, `to`, `x`, `y`, `width`, `height` | Crop image |
| `capture.monitors` | `act-capture` | — | List monitors (native backend) |
| `morph.meta` | `act-morph` | `path` | PDF meta (needs pdfium when enabled) |
| `morph.render` | `act-morph` | `path`; `offset?`, `scale?` | PDF page render |
| `morph.export` | `act-morph` | `src`, `dest` | PDF export |
| `morph.merge` | `act-morph` | `paths`, `dest` | Merge PDFs |
| `morph.split` | `act-morph` | `path`, `dir`; `limit?`, `ranges?` | Split PDF |

## Notable caveats

1. **`cron.schedule`** — Registered for schema/discovery completeness; calling it **errors** (`尚未实现`). Schedule externally (`systemd`, Task Scheduler, or call `corex run`).
2. **`compression.*` + `7z`** — Format is recognized but **not enabled** in the current build; use `zip` or `tar.gz`.
3. **Platform Actions** — `capture.*` (screenshot/monitors), `bootstrap.*` (env/force), and some `morph.*` paths may error when native backends / pdfium are unavailable.
4. **Runtime disable** — `[plugins].disabled` / `disabled_actions` in config can hide Actions after registration.
5. **Permissions** — When a Shortcut declares any permission flag, undeclared categories are denied (see [shortcut-yaml.md](./shortcut-yaml.md)).

## WASM plugins

Third-party `*.wasm` components may register additional IDs via discovery. Host bindgen is incomplete — failed loads are skipped. See [plugins/README.md](../plugins/README.md).

## Related

- [shortcut-yaml.md](./shortcut-yaml.md)
- [ipc-protocol.md](./ipc-protocol.md)
- [architecture.md](./architecture.md)
