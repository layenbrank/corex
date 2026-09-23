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

### 端点发现（宿主推荐）

daemon 会把「它到底监听在哪」写成 **`<数据目录>/endpoint.json`**，退出时删掉。
连接方按这个顺序找端点：

1. 显式配置（`socket_path` / `--socket`）
2. `<数据目录>/endpoint.json`——有 daemon 在跑时
3. 平台默认（上表）

```json
{
  "version": 1,
  "pid": 37692,
  "endpoint": "\\\\.\\pipe\\corex-disc-test",
  "kind": "pipe",
  "token_file": "C:\\Users\\Alice\\AppData\\Roaming\\corex\\data\\token"
}
```

`token_file` 只在 token 来自文件时出现；来自 `COREX_TOKEN` 或配置时**不写进记录**，连接方得自己去拿（那时也
**不要**去读 `<数据目录>/token`：那是上一个 daemon 留下的东西）。完整的四档顺序见
[IPC 协议 § 鉴权](../reference/IPC协议.md#鉴权)，Rust 侧就一个 `corex_ipc::find_token`。
字段表与「谁读谁不读」见 [IPC 协议 § 端点发现文件](../reference/IPC协议.md#端点发现文件)。

CLI 自己也走这条链路（`corex daemon status`、`corex run --remote`、`corex doctor` 的「IPC 端点」一行），
所以宿主不必在 JS / Python 里再复刻一遍 Windows `%APPDATA%` 与 XDG 的差异。
用 `COREX_DATA_DIR` 钉住数据目录时（随应用分发 corex 的常规做法），
双方看到的是同一个目录与同一份记录，见 [运行时配置 § 数据目录](../guide/运行时配置.md)。

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

| type              | 用途            | 主要字段                                              |
| ----------------- | --------------- | ----------------------------------------------------- |
| `ping`            | 存活检测        | —                                                     |
| `list_directives` | 列出指令        | `dir?` → `{name, path, bucket, summary, last_run?}[]` |
| `read_directive`  | 读一条指令      | `name`, `dir?`                                        |
| `save_directive`  | 保存一条指令    | `name`, `definition`, `dir?`                          |
| `list_runs`       | 最近执行记录    | `name?`, `limit?` → `{is_history_enabled, entries}`   |
| `list_actions`    | 列出 Action ID  | —                                                     |
| `run_directive`   | 执行指令        | `name`, `input?`（map）, `stream?`                    |
| `invoke`          | 调用单个 Action | `action`, `params?`, `stream?`                        |
| `shutdown`        | 关闭 Daemon     | —                                                     |

指令的读 / 写是给宿主编辑器用的：路径与 YAML 解析都在 daemon 里，宿主拿到 `text` 展示、
拿到 `definition` 编辑，改完原样交回即可（形状见 [IPC 协议](../reference/IPC协议.md)）。

执行历史同样只有 daemon 一份：`list_runs` 回引擎自己写的记录（新 → 旧，含失败），
`list_directives` 的条目顺带带上 `last_run`。宿主别再存一份「上次运行时间」——换台机器、
清过数据目录就会与它对不上。

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
{"type":"event","id":2,"progress":{"kind":"step_output","step":"build","action":"shell.run","stream":"stdout","text":"$ vite build\n"}}
{"type":"event","id":2,"progress":{"kind":"step_end","step":"copy","action":"file.copy","took_ms":31,"ok":true}}
{"type":"event","id":2,"progress":{"kind":"heartbeat","is_queued":false,"waited_ms":6000}}
```

要点：

- **读循环必须跳过 `event`**（按 `id` 归到对应请求，按 `kind` 分发），不能把第一个帧当成回答。
- 不置 `stream` 的请求**一帧都不会收到**：线格式与旧版完全一致，无需改客户端。
- 帧是**尽力而为**的：每条连接的待写队列（64 帧）满时 daemon 丢帧而不是等——
  进度不该把执行拖慢。所以不要拿帧当成“执行到哪了”的权威依据，`step_end` 系列可能缺。
- `unit` 为 `bytes` 或 `items`；`total` 未知时为 `null`。
- `heartbeat` 是**唯一不属于步骤的帧**（每两秒一帧）：`is_queued` 说它还在队列里等执行名额
  （见 `[daemon] max_jobs`），`waited_ms` 说等到现在多久了。**别用「总时长」判请求死没死**——
  一段几分钟的排队足够撞穿任何超时，而超时之后请求其实还在队列里、之后照样执行。
  用「静止期」判：收到任何帧（含心跳）就算它还活着。
- `step_output` 是**动作的文本输出**（`shell.run` / `exec.run` 的子进程 stdout / stderr）：
  `stream` 是 `stdout` / `stderr`，`text` 是**增量**原文——按到达顺序拼接才是完整输出，
  它可能含多行也可能半行断开，别按行解析；要完整读一次用终帧里动作的 `stdout` / `stderr`。
  没有这个帧，daemon 跑出来的指令输出就只留在 daemon 自己的控制台上（面板里只有步骤）。
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

### Node / Electron

用现成的客户端包：**[`packages/corex-client`](../../packages/corex-client/README.md)**（零依赖、无构建）。
它已经把「找端点、拿 token、进度帧不能当回答」这三件事做掉了：

```js
import { connect } from 'corex-client';

const corex = await connect({ spawn: true }); // 连不上就先拉起一个 daemon
const result = await corex.invoke(
  'file.copy',
  { from: 'a.bin', to: 'b.bin' },
  { onProgress: (frame) => console.log(frame.kind, frame.step) },
);
await corex.close();
```

### Python / 其他语言

1. 按上面的发现规则拿到端点（读 `endpoint.json`，或自己复刻平台默认）
2. 连接命名管道或 Unix socket
3. 按行读写 JSON
4. 每条请求附带 `auth_token`
5. **逐行读时跳过 `event` 帧**，直到拿到终帧

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
