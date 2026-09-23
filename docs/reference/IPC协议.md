# Corex IPC 协议（v10）

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

| 事项     | 行为                                                                                        |
| -------- | ------------------------------------------------------------------------------------------- |
| 建立连接 | **总是并发**：每条连接一个任务。一条慢请求不会让别的客户端连不上                            |
| 执行请求 | `run_directive` / `invoke` 受 **`[daemon] max_jobs`** 限制，超出的在队列里等                |
| 控制请求 | `ping` / `shutdown` / `list_directives` / `list_runs` / `list_actions` **不排队**，随时可答 |

`max_jobs`：`1` 串行 / `> 1`（默认 `4`）最多同时这么多 / `0` 不限。默认不串行是因为
宿主的「多任务并跑」（构建、拷贝、压缩之类互不相干的重活）本就互不干扰，串起来只会让后面几条
一直排队；要保住「两条指令不会同时驱鼠标键盘」这条 UI 自动化前提的宿主，**显式写
`max_jobs = 1`**，见 [运行时配置](../guide/运行时配置.md)。

排队中的请求**每两秒收到一帧心跳**（`stream: true` 时才有，见下）——排队期间流水线一帧不出，
客户端分不清「在等」与「死了」，而排队久到撞穿请求时限时，请求会被误报成失败
（它其实还在队列里，之后照样执行）。

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
| `list_directives` | `id`，`auth_token`，`dir?`                               | 列出指令（`{name, path, bucket, summary, last_run?}[]`；可选子目录；**路径沙箱**） |
| `read_directive`  | `id`，`auth_token`，`name`，`dir?`                       | 读一条指令的原文与模型                         |
| `save_directive`  | `id`，`auth_token`，`name`，`definition`，`dir?`         | 校验后写回，并回规范化之后的那份               |
| `list_runs`       | `id`，`auth_token`，`name?`，`limit?`                    | 最近的执行记录（新 → 旧），可按指令过滤        |
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

### 指令的列 / 读 / 写

宿主编辑器要展示并改指令，但**不该自己拼路径，也不该自己解析 YAML**：写盘格式（键序、
哪些默认值该省）只有引擎一份，解析口径也是。三条请求合起来就是完整的读写闭环：

| 请求              | 字段                                             | 回什么                                            |
| ----------------- | ------------------------------------------------ | ------------------------------------------------- |
| `list_directives` | `id`，`auth_token`，`dir?`                       | `{name, path, bucket, summary, last_run?}[]`，按名字排序 |
| `read_directive`  | `id`，`auth_token`，`name`，`dir?`               | `{name, path, text, definition}`                  |
| `save_directive`  | `id`，`auth_token`，`name`，`definition`，`dir?` | 同上，但 `text` 是**刚写下去**的那一份            |

```json
[
  {
    "name": "hello",
    "path": "C:\\Users\\Alice\\.corex\\directives\\hello.yaml",
    "bucket": "data",
    "summary": {
      "description": "打个招呼",
      "step_count": 2,
      "input_count": 1,
      "trigger_count": 0
    }
  }
]
```

| 字段                   | 说明                                                                                                         |
| ---------------------- | ------------------------------------------------------------------------------------------------------------ |
| `path`                 | 可交给外部编辑器的路径（已去掉 `\\?\` 前缀，分隔符按平台）                                                   |
| `bucket`               | 指令的分类，取 `system` / `network` / `data` / `ui` / `logic` / `plugin`；没写或**解析不了**时为 `null`      |
| `summary`              | 画卡片用的元信息；**文件解析不了时为 `null`**                                                                |
| `summary.description`  | 指令声明的描述，没写就是空串                                                                                 |
| `summary.step_count`   | 顶层步骤数（含 `parallel` / `steps` 这类复合步骤各算一步）                                                   |
| `summary.input_count`  | 声明的输入个数                                                                                               |
| `summary.trigger_count`| 声明的触发器个数                                                                                             |
| `last_run`             | 最近一次执行（`ok` / `duration_ms` / `error` / `run_count` / `failed_count`）；没跑过时不出现                |
| `text`                 | 文件原文，供宿主展示、以及保留自己没改的字段                                                                 |
| `definition`           | 解析后的 `Directive`（[指令 DSL](指令YAML.md)）；宿主编辑的就是它，改完原样交回 `save_directive`             |

`bucket` 与 `summary` 得先解析文件才知道，所以**解析不了的指令照样列出来**（编辑器要靠它把
文件打开去修），只是两者都为 `null`；`.yaml` / `.yml` 之外的同目录文件不是指令，不会出现在
列表里。列目录那次解析顺手把 `summary` 一起算出来，宿主画卡片不必把每个文件再读一遍。

`last_run` 也是这么顺出来的（见 [运行历史](#运行历史的只读出口)）：它没跑过、或 `[history]`
关掉时**整个字段不出现**——「没跑过」与「历史没开」是两件事，别混成 `null`。

`save_directive` 会先过一遍「动作是否注册」，再按引擎的序列化写盘：**被拒时一个字节也不写**
（模型解析不了 / 动作没注册 / 名字不是裸名 → 400，`dir` 越界 → 403），成功时用临时文件 +
原子替换，写坏一半的指令不会留在盘上。回给宿主的是规范化后的文本，与磁盘内容逐字节一致。

读一条指令时 `.yaml` 优先于 `.yml`；写回**已有的 `.yml`** 时仍写那个文件，不会另生一个
同名的 `.yaml`（否则谁生效就取决于扩展名先后了）。

### 运行历史的只读出口

宿主画「上次跑成什么样」不该自己攒一本账：历史文件（`history.jsonl`）由引擎在执行结束的
当口写，路径与开关都来自 `[history]` 配置（见
[数据目录与状态文件 § 目录内容](./数据目录与状态文件.md#2-目录内容)）。`list_runs` 把这份
账本读成 JSON，而 `list_directives` 的每条指令顺带带上自己的 `last_run` —— 列一次目录就够
画卡片，不必再逐条问一遍历史。

```json
{
  "is_history_enabled": true,
  "entries": [
    {
      "directive": "build-intern",
      "started_at_ms": 1763818203123,
      "ended_at_ms": 1763818204444,
      "ok": false,
      "error": "execution: 渲染失败: 变量 `missing` 未定义",
      "duration_ms": 1321
    }
  ]
}
```

| 字段                 | 说明                                                         |
| -------------------- | ------------------------------------------------------------ |
| `is_history_enabled` | `[history].enabled` 的现值                                   |
| `entries`            | 执行记录，**新 → 旧**；与 `history.jsonl` 里那些行逐字段一致 |

两个字段都得看：**关掉历史**与**一条都没跑过**都回空表，但卡片上一个该说「历史没开」，
另一个才说「从未运行」。`name` 只看一条指令，`limit` 限条数（不给时用 daemon 的默认 50 条）；
**失败也会记**，所以「上次失败成什么样」与 `corex history` 看到的是同一份事实。
`list_directives` 条目里的 `last_run` 是同一形状，另加窗口内的 `run_count` / `failed_count`。

### 进度帧（`stream: true`）

`run_directive` 与 `invoke` 多一个可选布尔 `stream`（默认 `false`）。置为 `true` 后，
daemon 会在**这条请求的终帧之前**插入零个或多个 `event` 帧：

```json
{"type":"event","id":3,"progress":{"kind":"step_start","seq":1,"step":"copy","action":"file.copy"}}
{"type":"event","id":3,"progress":{"kind":"step_progress","step":"copy","action":"file.copy","done":1048576,"total":41943040,"unit":"bytes"}}
{"type":"event","id":3,"progress":{"kind":"step_output","step":"build","action":"shell.run","stream":"stdout","text":"vite v8.0.11 building client environment for production...\n"}}
{"type":"event","id":3,"progress":{"kind":"step_end","step":"copy","action":"file.copy","took_ms":31,"ok":true}}
{"type":"event","id":3,"progress":{"kind":"heartbeat","is_queued":false,"waited_ms":6000}}
```

| 规则 | 说明                                                                                      |
| ---- | ----------------------------------------------------------------------------------------- |
| 顺序 | 帧一定排在 `ok` / `error` **之前**；`run_directive` 的帧与步骤同序                        |
| 归属 | 每帧带发起请求的 `id`；一条连接上可以有别的请求的帧在飞                                   |
| 可丢 | 帧是**尽力而为**的：每条连接的待写队列（64 帧）满时 daemon 丢帧而不是等。进度不该拖慢执行 |
| 关闭 | 不置 `stream` 的请求一帧也不会收到——旧客户端拿到的线与从前完全一致                        |

`unit` 为 `bytes`（复制 / 下载）或 `items`（递归删除）；`total` 未知时为 `null`。
`kind` 的四种步骤取值与 CLI 的 `corex run --json-events` 输出**同一套词汇**，宿主不必为
本地与远程两条路径记两套字段名。

`heartbeat` 是**第五种、也是唯一不属于步骤的帧**：从请求到达到它跑完，每两秒一帧。它只说一件事
——**我还活着**，以及**在排队还是在跑**：

| 字段        | 说明                                                                           |
| ----------- | ------------------------------------------------------------------------------ |
| `is_queued` | `true` = 还在队列里等执行名额（见 [并发模型](#并发模型)），`false` = 已经在跑   |
| `waited_ms` | 从请求到发出这帧等了多久：排队与执行都算在内                                    |

它存在是因为**排队与卡死在客户端看来一模一样**：两者都是「一段时间没有任何帧」，而一段几分钟的
排队足够撞穿宿主的请求时限——超时被报成失败、请求其实还排在队列里、之后照样执行（副作用真的发生
了）。所以判死该用**静止期**而不是总时长：收到**任何**帧都算它还活着，心跳尤其算。
心跳不参与渲染，也不会被重放进上报口（CLI 的 `Steps` / `Events` 渲染器直接忽略它）。

`step_output` 是**动作吐出来的文本**，目前只有 `shell.run` / `exec.run` 会产生：没有它，
子进程的 stdout / stderr 就只留在 daemon 自己的控制台上，宿主（Studio 的运行面板）一个字节
也看不到——面板里只会有步骤，没有命令输出。

| 字段     | 说明                                                                             |
| -------- | -------------------------------------------------------------------------------- |
| `stream` | `stdout` 或 `stderr`；宿主据此决定给「普通」还是「错误」着色                       |
| `text`   | **已解码**的原文，增量：按到达顺序拼接才是完整输出                                |

`text` 的切点由动作的读缓冲决定（`process_launch` 每次最多 8 KiB），所以它可能含多行，
也可能在半行处断开——**不要把它当「一行」**，更不要按行去解析。它不做去重、不补换行；
队列满时与其余帧一样会被丢掉（所以任何按帧拼输出的展示都只应当作「尽力而为的实时回显」，
要完整读取请用终帧里动作的返回值，如 `stdout` / `stderr` 字段）。

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
- [数据目录与状态文件](数据目录与状态文件.md) — `endpoint.json` / `token` 文件的生命周期
- [退出码与错误码](退出码与错误码.md) — `RpcError` 码与 CLI 退出码的对应
- [integration/Tauri接入指南.md](../integration/Tauri接入指南.md) — Sidecar 客户端
- [architecture.md](架构.md) — Workspace 概览
