# Corex IPC 协议（v5）

> **接入指南（推荐）：** [integration/IPC接入指南.md](../integration/IPC接入指南.md)

客户端（`corex` CLI、Tauri 等）与 **`corex-daemon`** 之间使用换行分隔的 JSON（NDJSON）。

事实来源：[`crates/ipc/src/protocol.rs`](../../crates/ipc/src/protocol.rs)。

## 帧格式

| 规则 | 取值 |
|------|------|
| 编码 | UTF-8 JSON，每行一条消息，以 `\n` 结尾 |
| 单行上限 | **`MAX_LINE_BYTES = 1_048_576`（1 MiB）** |
| 方向 | 客户端 → daemon：`Request`；daemon → 客户端：`Response` |
| 判别字段 | Serde `tag = "type"`，`rename_all = "snake_case"` |

超长或畸形行由传输/处理层拒绝（不要依赖部分解析）。

## 传输端点

| 平台 | 默认端点 | 覆盖方式 |
|------|----------|----------|
| Linux / macOS | `<data-dir>/corex.sock`（Unix domain socket） | `corex-daemon --socket <path>` 或 `[daemon].socket_path` |
| Windows | `\\.\pipe\corex`（Named Pipe） | `--socket` / `--pipe` 等价路径，或配置项 |

相对 `socket_path` 解析到平台数据目录下。Windows 管道路径（`\\.\pipe\...` 或 `//./pipe/...`）原样使用。

二进制名为 **`corex-daemon`**（不是 `corex-serve`）。

## 鉴权

每个请求可带 `auth_token`（schema 上可选；daemon 配置了 token 时**实际必填**）。

Daemon 端 token 解析顺序：

1. 环境变量 **`COREX_TOKEN`**（非空）
2. 配置 `[daemon].token`（非空）
3. 文件 **`<data-dir>/token`** — 已有则读取，否则创建随机 32 字节 hex（Unix 模式 `0600`）

CLI 客户端加载 `COREX_TOKEN` 或 `<data-dir>/token`，经 `Request::with_auth_token` 附带。不匹配 → `Response::Error`，code **401**。

见 [`config/corex.toml`](../../config/corex.toml) 中 `[daemon]` 注释。

## 请求类型

所有变体共享可选 `id`（默认 `0`）与可选 `auth_token`。

| `type` | 字段 | 用途 |
|--------|------|------|
| `ping` | `id`，`auth_token` | 探活 |
| `shutdown` | `id`，`auth_token` | 优雅退出 daemon |
| `list_directives` | `id`，`auth_token`，`dir?` | 列出指令名（可选子目录；**路径沙箱**） |
| `list_actions` | `id`，`auth_token` | 列出已注册 Action ID |
| `run_directive` | `id`，`auth_token`，`name`，`input?`，`path?` | 按名运行指令，或路径（限制在 directives 根下） |
| `invoke` | `id`，`auth_token`，`action`，`params?` | 按 ID 调用单个 Action |

### 示例

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

**相对 v3：** 不再有 `module` + 嵌套 `action` 线格式。使用单个 Action ID 字符串（如 `capture.screenshot`）。

## 响应类型

| `type` | 字段 | 含义 |
|--------|------|------|
| `pong` | `id` | 对 `ping` 的回复 |
| `ok` | `id`，`data` | 成功；`data` 为 Corex `Value`（JSON） |
| `error` | `id`，`error: { code, message }` | 失败 |
| `bye` | `id` | 对 `shutdown` 的回复（daemon 退出中） |

### `RpcError` 代码（辅助）

| Code | Helper | 典型用途 |
|------|--------|----------|
| 400 | `invalid` | 参数/请求错误 |
| 401 | `unauthorized` | 缺少或错误的 auth token |
| 403 | `forbidden` | 拒绝 |
| 404 | `not_found` | 未知指令 / Action |
| 500 | `internal` | 未预期失败 |

### 示例

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

## 路径沙箱

对带 `path` 的 `run_directive` 与带 `dir` 的 `list_directives`，daemon 在配置的 directives 根下解析路径，并**拒绝**逃逸（`confine_under`）。指令 `name` 必须是裸名（无 `..`、`/`、`\` 或绝对路径）。

## 相关文档

- [actions.md](内置Action.md) — Action ID
- [directive-yaml.md](指令YAML.md) — Directive DSL
- [integration/Tauri接入指南.md](../integration/Tauri接入指南.md) — Sidecar 客户端
- [architecture.md](架构.md) — Workspace 概览
