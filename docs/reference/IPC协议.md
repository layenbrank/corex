# Corex IPC 协议（v9）

> **接入指南（推荐）：** [integration/IPC接入指南.md](../integration/IPC接入指南.md)

客户端（`corex` CLI、Tauri 等）与 **`corex-daemon`** 之间使用换行分隔的 JSON（NDJSON）。

事实来源：[`crates/ipc/src/protocol.rs`](../../crates/ipc/src/protocol.rs)。

## 帧格式

| 规则     | 取值                                                    |
| -------- | ------------------------------------------------------- |
| 编码     | UTF-8 JSON，每行一条消息，以 `\n` 结尾                  |
| 单行上限 | **`MAX_LINE_BYTES = 1_048_576`（1 MiB）**               |
| 方向     | 客户端 → daemon：`Request`；daemon → 客户端：`Response` |
| 判别字段 | Serde `tag = "type"`，`rename_all = "snake_case"`       |

超长或畸形行由传输/处理层拒绝（不要依赖部分解析）。

## 传输端点

| 平台          | 默认端点                                      | 覆盖方式                                                 |
| ------------- | --------------------------------------------- | -------------------------------------------------------- |
| Linux / macOS | `<data-dir>/corex.sock`（Unix domain socket） | `corex-daemon --socket <path>` 或 `[daemon].socket_path` |
| Windows       | `\\.\pipe\corex`（Named Pipe）                | `--socket` / `--pipe` 等价路径，或配置项                 |

相对 `socket_path` 解析到平台数据目录下。Windows 管道路径（`\\.\pipe\...` 或 `//./pipe/...`）原样使用。

**省略 `socket_path` 即平台默认端点**，这也是跨平台共用一份配置时唯一安全的写法。
Windows 上的端点只能是命名管道：给一个 Unix 风格的文件路径（如 `corex.sock`）会在
启动时被直接拒绝，不会静默回退。

二进制名为 **`corex-daemon`**（不是 `corex-serve`）。

### 端点发现文件

daemon 在开始服务**之前**把「它到底监听在哪」写成 **`<data-dir>/endpoint.json`**，退出时删掉。
连接方读到的是事实而不是约定——不必再复刻一遍平台目录规则，也不必猜配置里改没改端点。

```json
{
  "version": 1,
  "pid": 37692,
  "endpoint": "\\\\.\\pipe\\corex-disc-test",
  "kind": "pipe",
  "token_file": "C:\\Users\\Alice\\AppData\\Roaming\\corex\\data\\token"
}
```

| 字段         | 说明                                                                                           |
| ------------ | ---------------------------------------------------------------------------------------------- |
| `version`    | 格式版本；读方不认识就**忽略整份文件**（退回平台默认，而不是按旧字段猜）                       |
| `pid`        | 监听方进程号，仅供排错；**不做存活性判断**（pid 会被复用）                                      |
| `endpoint`   | 实际监听的端点                                                                                   |
| `kind`       | `pipe` 或 `socket`；连接方据此挑连接 API，不必自己判平台                                          |
| `token_file` | token **文件**的位置；**仅当** token 来自文件时才有（见下）                                      |

`token_file` 在 token 来自 `COREX_TOKEN` 或配置 `[daemon].token` 时**不出现**：那两处的值属于调用方，
不该被复制进一个默认权限的文件。此时连接方得自己去拿 token。

**谁读、谁不读：**

| 角色             | 顺序                                                    |
| ---------------- | -------------------------------------------------------- |
| 连接方（CLI / 宿主 / SDK） | 显式配置（`socket_path` / `--socket`）→ **本文件** → 平台默认 |
| 监听方（daemon） | 显式配置 → 平台默认；**从不读本文件**                     |

监听方不读是有意的：崩溃残留的记录会把新起的 daemon 带到上一个进程的端点上。
连接方看到的是「运行中 daemon 的事实」，所以发现排在平台默认之前——但**排在显式配置之后**，
命令行与配置里写死的东西必须赢过发现。

⚠️ 记录可能是**残留**的：daemon 被强杀时来不及删。「有记录」不等于「daemon 在跑」，
那件事的问法是去发一条 `ping`。

## 并发模型

连接与执行是两件事，规则不同——宿主据此决定要不要开多条连接：

| 事项     | 行为                                                                     |
| -------- | ------------------------------------------------------------------------ |
| 建立连接 | **总是并发**：每条连接一个任务。一条慢请求不会让别的客户端连不上            |
| 执行请求 | `run_directive` / `invoke` 受 **`[daemon] max_jobs`** 限制，超出的在队列里等 |
| 控制请求 | `ping` / `shutdown` / `list_directives` / `list_actions` **不排队**，随时可答 |

`max_jobs`：`1`（默认）串行 / `> 1` 最多同时这么多 / `0` 不限。默认取 `1` 是为了保住
「两条指令不会同时驱鼠标键盘」这条 UI 自动化的前提——**并行是显式选择**，见
[运行时配置](../guide/运行时配置.md)。

对请求本身的两条推论：

- **一条连接上可以同时有多个请求在飞**：每条带自己的 `id`，回话按 `id` 归位。
  帧（`event`）也带 `id`，所以并发时不会串到别的请求上。
- 排队的请求**不消耗连接**：它只是还没开始执行，客户端不要用连接是否建立来判断「有没有开始跑」。
  想知道进度就置 `stream: true`，第一个 `step_start` 才算真的开始了。

## 鉴权

每个请求可带 `auth_token`（schema 上可选；daemon 配置了 token 时**实际必填**）。

Daemon 端 token 解析顺序：

1. 环境变量 **`COREX_TOKEN`**（非空）
2. 配置 `[daemon].token`（非空）
3. 文件 **`<data-dir>/token`** — 已有则读取，否则创建随机 32 字节 hex（Unix 模式 `0600`）

CLI / 宿主 / SDK 侧的 token 解析顺序（一处实现：`corex_ipc::find_token`）：

1. 显式选项（CLI：`COREX_TOKEN`；SDK：`connect({ token })`）
2. 配置 `[daemon].token`
3. 端点记录里的 `token_file`——daemon 自己说“它的 token 在哪个文件”
4. `<data-dir>/token`（只在**没有**端点记录时才看：daemon 还没起过）

拿到的 token 经 `Request::with_auth_token` 附带。不匹配 → `Response::Error`，code **401**。

⚠️ 记录在、但里面没有 `token_file` 时**不读** `<data-dir>/token`：那说明 daemon 的 token
来自第 1 / 2 档（值属于调用方，不会被复制进记录），而那个文件只属于**上一个** daemon。

见 [`config/corex.toml`](../../config/corex.toml) 中 `[daemon]` 注释。

## 请求类型

所有变体共享可选 `id`（默认 `0`）与可选 `auth_token`。

| `type`            | 字段                                                     | 用途                                           |
| ----------------- | -------------------------------------------------------- | ---------------------------------------------- |
| `ping`            | `id`，`auth_token`                                       | 探活                                           |
| `shutdown`        | `id`，`auth_token`                                       | 优雅退出 daemon                                |
| `list_directives` | `id`，`auth_token`，`dir?`                               | 列出指令名（可选子目录；**路径沙箱**）         |
| `list_actions`    | `id`，`auth_token`                                       | 动作目录文档（参数表、权限与 `inputSchema`）   |
| `run_directive`   | `id`，`auth_token`，`name`，`input?`，`path?`，`stream?` | 按名运行指令，或路径（限制在 directives 根下） |
| `invoke`          | `id`，`auth_token`，`action`，`params?`，`stream?`       | 按 ID 调用单个 Action                          |

### `list_actions` 的动作目录

`list_actions` 回的是**整份目录文档**（`corex actions --json` 的那一份），每个动作是**完整描述**
——不是只有 id。宿主与 agent 需要知道「怎么调」：参数类型、默认值与要声明的权限。
两边的形状完全一致（同一个 `corex_registry::catalog::document`）。

```json
{
  "type": "ok",
  "id": 2,
  "data": {
    "version": "10.0.0",
    "count": 80,
    "bucket": null,
    "actions": [
      {
        "id": "file.copy",
        "name": "文件复制",
        "description": "复制文件（单文件，可上报分块进度）",
        "bucket": "data",
        "params": [
          { "name": "from", "ty": "file", "required": true },
          { "name": "to", "ty": "file", "required": true }
        ],
        "tags": [],
        "permissions": ["filesystem"],
        "input_schema": {
          "type": "object",
          "properties": {
            "from": { "type": "string", "format": "path" },
            "to": { "type": "string", "format": "path" }
          },
          "required": ["from", "to"]
        }
      }
    ]
  }
}
```

| 字段 | 说明 |
| ---- | ---- |
| `version` | 产出这份目录的 corex 版本；宿主据此判断手里的参数表要不要重拉 |
| `bucket` | 筛选条件原样回报；daemon 不筛选，所以恒为 `null` |
| `params[].ty` | Corex 自己的类型标签（`str` / `file` / `map` / `any` …）；`description` 与 `default` 为 `None` 时**不出现** |
| `input_schema` | 同一批事实派生出的 JSON Schema，可直接当 MCP 工具的 `inputSchema` 用 |
| `permissions` | 取自动作自己的声明（`Action::permissions`），不是另抄的一张表 |

⚠️ **v9.0.0 → 下一版有一处破坏性变更**：`data` 从「动作数组」变成「目录文档」（多了一层
`actions`）。只要 id 的客户端从 `data[].id` 改成 `data.actions[].id` 即可；
[`packages/corex-client`](../../packages/corex-client/README.md) 的 `actions()` 已经替你拆好了。

### 进度帧（`stream: true`）

`run_directive` 与 `invoke` 多一个可选布尔 `stream`（默认 `false`）。置为 `true` 后，
daemon 会在**这条请求的终帧之前**插入零个或多个 `event` 帧：

```json
{"type":"event","id":3,"progress":{"kind":"step_start","seq":1,"step":"copy","action":"file.copy"}}
{"type":"event","id":3,"progress":{"kind":"step_progress","step":"copy","action":"file.copy","done":1048576,"total":41943040,"unit":"bytes"}}
{"type":"event","id":3,"progress":{"kind":"step_end","step":"copy","action":"file.copy","took_ms":31,"ok":true}}
```

| 规则 | 说明                                                                                      |
| ---- | ----------------------------------------------------------------------------------------- |
| 顺序 | 帧一定排在 `ok` / `error` **之前**；`run_directive` 的帧与步骤同序                        |
| 归属 | 每帧带发起请求的 `id`；一条连接上可以有别的请求的帧在飞                                   |
| 可丢 | 帧是**尽力而为**的：每条连接的待写队列（64 帧）满时 daemon 丢帧而不是等。进度不该拖慢执行 |
| 关闭 | 不置 `stream` 的请求一帧也不会收到——旧客户端拿到的线与从前完全一致                        |

`unit` 为 `bytes`（复制 / 下载）或 `items`（递归删除）；`total` 未知时为 `null`。
`kind` 的三种取值与 CLI 的 `corex run --json-events` 输出**同一套词汇**，宿主不必为
本地与远程两条路径记两套字段名。

### 示例

```json
{ "type": "ping", "id": 1, "auth_token": "<token>" }
```

```json
{ "type": "list_actions", "id": 2, "auth_token": "<token>" }
```

```json
{
  "type": "run_directive",
  "id": 3,
  "auth_token": "<token>",
  "name": "hello",
  "input": { "who": "Corex" }
}
```

```json
{
  "type": "invoke",
  "id": 4,
  "auth_token": "<token>",
  "action": "capture.screenshot",
  "params": { "to": "/tmp/shot.png" },
  "stream": true
}
```

```json
{ "type": "shutdown", "id": 5, "auth_token": "<token>" }
```

**相对 v3：** 不再有 `module` + 嵌套 `action` 线格式。使用单个 Action ID 字符串（如 `capture.screenshot`）。

## 响应类型

| `type`  | 字段                             | 含义                                             |
| ------- | -------------------------------- | ------------------------------------------------ |
| `pong`  | `id`                             | 对 `ping` 的回复                                 |
| `ok`    | `id`，`data`                     | 成功；`data` 为 Corex `Value`（JSON）            |
| `error` | `id`，`error: { code, message }` | 失败                                             |
| `event` | `id`，`progress`                 | **中间帧**（进度）；仅在请求置了 `stream` 时出现 |
| `bye`   | `id`                             | 对 `shutdown` 的回复（daemon 退出中）            |

### `RpcError` 代码（辅助）

| Code | Helper         | 典型用途                |
| ---- | -------------- | ----------------------- |
| 400  | `invalid`      | 参数/请求错误           |
| 401  | `unauthorized` | 缺少或错误的 auth token |
| 403  | `forbidden`    | 拒绝                    |
| 404  | `not_found`    | 未知指令 / Action       |
| 500  | `internal`     | 未预期失败              |

### 示例

```json
{ "type": "pong", "id": 1 }
```

```json
{ "type": "ok", "id": 4, "data": { "path": "/tmp/shot.png" } }
```

```json
{ "type": "error", "id": 4, "error": { "code": 401, "message": "unauthorized" } }
```

```json
{ "type": "bye", "id": 5 }
```

## 路径沙箱

对带 `path` 的 `run_directive` 与带 `dir` 的 `list_directives`，daemon 在配置的 directives 根下解析路径，并**拒绝**逃逸（`confine_under`）。指令 `name` 必须是裸名（无 `..`、`/`、`\` 或绝对路径）。

## 相关文档

- [actions.md](内置Action.md) — Action ID
- [directive-yaml.md](指令YAML.md) — Directive DSL
- [integration/Tauri接入指南.md](../integration/Tauri接入指南.md) — Sidecar 客户端
- [architecture.md](架构.md) — Workspace 概览
