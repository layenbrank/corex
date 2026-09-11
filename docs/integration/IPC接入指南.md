# IPC 接入指南

本文说明如何通过 **NDJSON** 与 **`corex-daemon`** 通信，从任意语言/进程调用指令或单个 Action。

英文协议细节（字段级）：[ipc-protocol.md](../reference/IPC协议.md)

---

## 1. 启动 Daemon

```powershell
# 前台（调试）
corex daemon run

# 或独立二进制
corex-daemon

# 后台（CLI 封装）
corex daemon start
corex daemon status
corex daemon stop
```

---

## 2. 连接端点

| 平台          | 默认端点                            |
| ------------- | ----------------------------------- |
| Windows       | 命名管道 `\\.\pipe\corex`           |
| Linux / macOS | Unix socket `<数据目录>/corex.sock` |

覆盖方式：

- CLI：`corex-daemon --socket <path>`
- 配置：`[daemon] socket_path = "..."`

---

## 3. 鉴权（必做）

每个请求应带 **`auth_token`**。

Token 解析顺序（Daemon 端）：

1. 环境变量 **`COREX_TOKEN`**
2. `config.toml` 的 `[daemon] token`
3. 文件 **`<数据目录>/token`**（不存在则自动生成 32 字节 hex）

客户端读取方式与 CLI 相同：优先 `COREX_TOKEN`，否则读 `token` 文件。

Token 不匹配 → 响应 `error`，code **401**。

---

## 4. 消息格式

- 编码：**UTF-8 JSON**
- 分帧：一行一条消息，以 `\n` 结尾
- 大小：单行最大 **1 MiB**

### 请求（Client → Daemon）

公共字段：`type`、`id`（可选，默认 0）、`auth_token`

| type              | 用途            | 主要字段                           |
| ----------------- | --------------- | ---------------------------------- |
| `ping`            | 存活检测        | —                                  |
| `list_directives` | 列出指令名      | `dir?`                             |
| `list_actions`    | 列出 Action ID  | —                                  |
| `run_directive`   | 执行指令        | `name`, `input?`（map）, `stream?` |
| `invoke`          | 调用单个 Action | `action`, `params?`, `stream?`     |
| `shutdown`        | 关闭 Daemon     | —                                  |

### 响应（Daemon → Client）

| type    | 说明                                                   |
| ------- | ------------------------------------------------------ |
| `ok`    | 成功，`data` 为结果 Value                              |
| `error` | 失败，`error: { code, message }`（嵌套对象）           |
| `event` | **中间帧**（进度）；仅在请求置了 `stream: true` 时出现 |

### 流式进度

长耗时的指令靠 `ok` 回一个结果，中间是看不到任何东西的。置 `stream: true` 后，daemon 会
在这条请求期间插入零个或多个 `event` 帧（一定排在终帧之前）：

```json
{"type":"event","id":2,"progress":{"kind":"step_start","seq":1,"step":"copy","action":"file.copy"}}
{"type":"event","id":2,"progress":{"kind":"step_progress","step":"copy","action":"file.copy","done":1048576,"total":41943040,"unit":"bytes"}}
{"type":"event","id":2,"progress":{"kind":"step_end","step":"copy","action":"file.copy","took_ms":31,"ok":true}}
```

要点：

- **读循环必须跳过 `event`**（按 `id` 归到对应请求，按 `kind` 分发），不能把第一个帧当成回答。
- 不置 `stream` 的请求**一帧都不会收到**：线格式与旧版完全一致，无需改客户端。
- 帧是**尽力而为**的：每条连接的待写队列（64 帧）满时 daemon 丢帧而不是等——
  进度不该把执行拖慢。所以不要拿帧当成“执行到哪了”的权威依据，`step_end` 系列可能缺。
- `unit` 为 `bytes` 或 `items`；`total` 未知时为 `null`。
- Rust 侧：`Transport::send_events(&mut self, &request, &sink)` 帮你把帧路由到 `sink`；
  `corex_ipc::Replay` 是一个现成的落点，把帧重放进一个 `corex_core::progress::Observer`。

---

## 5. 示例

### Ping

```json
{ "type": "ping", "id": 1, "auth_token": "<your-token>" }
```

### 执行指令

```json
{
  "type": "run_directive",
  "id": 2,
  "auth_token": "<your-token>",
  "name": "hello",
  "input": { "who": "Corex" }
}
```

### 调用单个 Action

```json
{
  "type": "invoke",
  "id": 3,
  "auth_token": "<your-token>",
  "action": "generate.uuid",
  "params": { "count": 1 }
}
```

---

## 6. 各语言接入要点

### Rust

- Crate：**`corex-ipc`**（`Transport`、`Request`、`Response`）
- 参考：`examples/tauri/corex_ipc.rs`

### Node / Python / 其他

1. 连接命名管道或 Unix socket
2. 按行读写 JSON
3. 每条请求附带 `auth_token`

Windows 命名管道示例（概念）：

```python
# 伪代码：使用 win32pipe 或 asyncio open_connection('\\\\.\\pipe\\corex')
send_line(json.dumps({"type": "ping", "auth_token": token}) + "\n")
response = read_line()
```

---

## 7. 与 CLI 的关系

`corex run` **默认不经过 Daemon**，在进程内直接加载引擎；`corex run --remote` 则把指令
交给 Daemon 执行（此时进度通过上面的 `event` 帧流回 CLI 并渲染）。仅当你需要从**其他进程**
复用已加载的 registry、插件或统一审计时，才需要 Daemon + IPC。

Tauri 等桌面壳：**推荐 Daemon sidecar 模式**。

---

## 8. 故障排查

| 现象       | 处理                                         |
| ---------- | -------------------------------------------- |
| 连接拒绝   | `corex daemon status`；检查 pipe/socket 路径 |
| 401        | 对齐 `COREX_TOKEN` 或 `token` 文件           |
| 指令未找到 | `list_directives`；确认 `directives/` 目录   |
| 行过大     | 拆分结果或避免在单步返回超大 body            |

---

## 相关文档

- [接入总览](./接入总览.md)
- [Tauri 接入指南](./Tauri接入指南.md)
- [运行时配置](../guide/运行时配置.md)
- [ipc-protocol.md](../reference/IPC协议.md)
