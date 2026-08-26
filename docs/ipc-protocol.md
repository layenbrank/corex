# Corex IPC Protocol (v4)

Newline-delimited JSON (NDJSON) between clients (`corex` CLI, Tauri, etc.) and **`corex-daemon`**.

Source of truth: [`crates/ipc/src/protocol.rs`](../crates/ipc/src/protocol.rs).

## Framing

| Rule | Value |
|------|--------|
| Encoding | UTF-8 JSON, one message per line, terminated by `\n` |
| Max line size | **`MAX_LINE_BYTES = 1_048_576` (1 MiB)** |
| Direction | Client → daemon: `Request`; daemon → client: `Response` |
| Discriminator | Serde `tag = "type"`, `rename_all = "snake_case"` |

Oversized or malformed lines are rejected by the transport/handler (do not rely on partial parses).

## Transport endpoints

| Platform | Default endpoint | Override |
|----------|------------------|----------|
| Linux / macOS | `<data-dir>/corex.sock` (Unix domain socket) | `corex-daemon --socket <path>` or `[daemon].socket_path` |
| Windows | `\\.\pipe\corex` (Named Pipe) | `--socket` / `--pipe` equivalent path, or config |

Relative `socket_path` values resolve under the platform data directory. Windows pipe paths (`\\.\pipe\...` or `//./pipe/...`) are used as-is.

Binary name is **`corex-daemon`** (not `corex-serve`).

## Authentication

Every request may include `auth_token` (optional in the schema; **required in practice** when the daemon has a token).

Token resolution order (daemon):

1. Environment variable **`COREX_TOKEN`** (non-empty)
2. Config `[daemon].token` (non-empty)
3. File **`<data-dir>/token`** — read existing, or create a random 32-byte hex secret (mode `0600` on Unix)

CLI clients load `COREX_TOKEN` or `<data-dir>/token` and attach it via `Request::with_auth_token`. Mismatch → `Response::Error` with code **401**.

See [`config/corex.toml`](../config/corex.toml) comments under `[daemon]`.

## Request types

All variants share optional `id` (default `0`) and optional `auth_token`.

| `type` | Fields | Purpose |
|--------|--------|---------|
| `ping` | `id`, `auth_token` | Liveness |
| `shutdown` | `id`, `auth_token` | Graceful daemon exit |
| `list_directives` | `id`, `auth_token`, `dir?` | List Directive names (optional subdir under directives root; **path-confined**) |
| `list_actions` | `id`, `auth_token` | List registered Action IDs |
| `run_directive` | `id`, `auth_token`, `name`, `input?`, `path?` | Run a Directive by name, or by path confined under the directives directory |
| `invoke` | `id`, `auth_token`, `action`, `params?` | Invoke a single Action by ID |

### Examples

```json
{"type":"ping","id":1,"auth_token":"<token>"}
```

```json
{"type":"list_actions","id":2,"auth_token":"<token>"}
```

```json
{"type":"run_directive","id":3,"auth_token":"<token>","name":"hello","input":{"who":"Corex"}}
```

```json
{"type":"invoke","id":4,"auth_token":"<token>","action":"capture.screenshot","params":{"to":"/tmp/shot.png"}}
```

```json
{"type":"shutdown","id":5,"auth_token":"<token>"}
```

**v3 note:** There is no `module` + nested `action` wire format. Use a single Action ID string (e.g. `capture.screenshot`).

## Response types

| `type` | Fields | Meaning |
|--------|--------|---------|
| `pong` | `id` | Reply to `ping` |
| `ok` | `id`, `data` | Success; `data` is a Corex `Value` (JSON) |
| `error` | `id`, `error: { code, message }` | Failure |
| `bye` | `id` | Reply to `shutdown` (daemon exiting) |

### `RpcError` codes (helpers)

| Code | Helper | Typical use |
|------|--------|-------------|
| 400 | `invalid` | Bad params / bad request |
| 401 | `unauthorized` | Missing/wrong auth token |
| 403 | `forbidden` | Denied |
| 404 | `not_found` | Unknown Directive / action |
| 500 | `internal` | Unexpected failure |

### Examples

```json
{"type":"pong","id":1}
```

```json
{"type":"ok","id":4,"data":{"path":"/tmp/shot.png"}}
```

```json
{"type":"error","id":4,"error":{"code":401,"message":"unauthorized"}}
```

```json
{"type":"bye","id":5}
```

## Path confinement

For `run_directive` with `path` and `list_directives` with `dir`, the daemon resolves paths under the configured directives root and **rejects traversal** outside that root (`confine_under`). Directive `name` must be a bare name (no `..`, `/`, `\`, or absolute paths).

## Related

- [actions.md](./actions.md) — Action IDs
- [directive-yaml.md](./directive-yaml.md) — Directive DSL
- [tauri-integration.md](./tauri-integration.md) — Sidecar client
- [architecture.md](./architecture.md) — Workspace overview
