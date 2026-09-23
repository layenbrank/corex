# corex-client

Corex daemon 的 NDJSON IPC 客户端。**零依赖、零构建**：`import` 的就是 `src/index.js` 本身，
类型在 `src/index.d.ts`。

它替宿主做三件过去只能各写一遍的事：**该连哪个端点**、**token 从哪来**、**进度帧与终帧怎么分**。

## 安装

还没发布到 npm。宿主可以复制 `src/` 进项目（只有两个文件），或在自己 `package.json` 里写：

```json
{ "dependencies": { "corex-client": "file:../path/to/packages/corex-client" } }
```

**宿主示例：** [`examples/electron/`](../../examples/electron/README.md)——含 sidecar 拉起、
`contextBridge` 桥、退出时收掉 daemon，以及一个不开 Electron 也能跑的宿主侧检查脚本。

## 快速开始

```js
import { connect } from 'corex-client'

// 连已经跑着的 daemon；没有就先拉起一个（宿主启动时的常规写法）
const corex = await connect({ spawn: true })

await corex.ping()

const catalog = await corex.actions() // 与 `corex actions --json` 逐字同形
const copy = catalog.find((action) => action.id === 'file.copy')

const result = await corex.invoke(
  'file.copy',
  { from: 'a.bin', to: 'b.bin' },
  { onProgress: (frame) => console.log(frame.kind, frame) }
)

await corex.run('hello', { input: { who: 'electron' } })

await corex.close() // 自己拉起的 daemon 会一并收掉
```

## 端点发现

| 顺序 | 来源                                                               |
| ---- | ------------------------------------------------------------------ |
| 1    | `connect({ endpoint })`                                            |
| 2    | `<数据目录>/endpoint.json`（daemon 运行中写下）                    |
| 3    | 平台默认（Windows `\\.\pipe\corex`；Unix `<数据目录>/corex.sock`） |

`discover()` 会把结果与来源（`option` / `record` / `default`）一起给你，排错时第一个该看它。

⚠️ **数据目录**：`resolveDataDir` 是 `COREX_DATA_DIR` → 平台项目目录，但 Rust 的 `data_dir()`
**还有一档「二进制所在目录（可写时）」**，而这里不知道 `corex.exe` 在哪，复刻不了。
所以随宿主分发 corex 时**请显式设 `COREX_DATA_DIR`**（`spawnDaemon` 会替你设给子进程），
否则父子可能各看一份数据目录，表现成最难查的那种「连不上」。

## token

`connect({ token })` → `COREX_TOKEN` → 记录里的 `token_file` → `<数据目录>/token`。
与 Rust 侧 `corex_ipc::find_token` 是同一套顺序（CLI 也走它）。

记录在、但里面没有 `token_file` 时**不读** `<数据目录>/token`：那说明 daemon 的 token 来自
`COREX_TOKEN` 或配置（属于调用方，不会被复制进记录），而那个文件只属于**上一个** daemon。

来自**配置文件**（`[daemon].token`）的 token 这里看不到：Rust 侧读 TOML，这个包刻意不引
TOML 解析器。那种部署请把值显式传给 `connect({ token })` 或设 `COREX_TOKEN`。

## 进度帧

置了 `stream` 的请求会先收到零个或多个 `event` 帧，终帧（`ok` / `error`）排在最后。
**不能把第一个帧当成回答**；按 `id` 归位是唯一正确的读法。这件事已经在 `CorexClient` 里做掉了：
给 `onProgress` 就替你分流，不给就一个字节的进度都不传（不置 `stream`，daemon 那边也省掉观察者）。

`frame.kind` 是 `step_start` / `step_progress` / `step_output` / `step_end` / `heartbeat`，与
`corex run --json-events` **同一套词汇**，宿主不必为本地与远程两条路径记两套字段名。

`heartbeat` 是排队等待的心跳：只带 `is_queued` 与 `waited_ms`，**没有 `step` / `action`**。
按 `kind` 判窄再取字段（`step` 等只在另外四种帧上存在），否则排队的请求会在第一帧就炸。

`step_output` 是动作吐出来的文本（`shell.run` / `exec.run` 的子进程 stdout / stderr），
`stream` 告诉你是哪个流、`text` 是**增量**原文：按到达顺序拼接才是完整输出，它可能含多行、
也可能半行断开，**不要按行解析**。它与别的帧一样是尽力而为的（队列满会丢），
要完整读一次用终帧里动作的 `stdout` / `stderr` 字段。

## API

| 成员                                                 | 说明                                               |
| ---------------------------------------------------- | -------------------------------------------------- |
| `connect(options)` / `CorexClient.connect`           | 建连；`spawn: true` 时连不上先拉起 daemon          |
| `client.ping()`                                      | 探活（`corex daemon status` 用的同一个请求）       |
| `client.actions()`                                   | 动作目录：参数表 + 权限 + `input_schema`           |
| `client.directives(dir?)`                            | 指令名（受路径沙箱约束）                           |
| `client.invoke(action, params, { onProgress })`      | 调用单个 Action                                    |
| `client.run(name, { input, file, onProgress })`      | 执行指令                                           |
| `client.shutdown()` / `client.close({ stopDaemon })` | 请 daemon 退出 / 断开（自己起的默认顺带收掉）      |
| `client.daemon`                                      | 自己拉起的子进程句柄；连别人的时是 `null`          |
| `discover(options)`                                  | 只解析「连到哪、用什么 token」，便于打进日志       |
| `spawnDaemon(options)`                               | 只拉起 daemon，返回 `{ child, stop, kill }`        |
| `RpcError`                                           | 带 `code`（400 / 401 / 403 / 404 / 500）的请求失败 |

`invoke` / `run` **默认不限时**：一条指令跑几分钟是正常的，给它卡上限只会把正常工作判成失败
（连接真断了由 `close` / `error` 拦下）。要上限就自己传 `timeoutMs`。

## 测试

```powershell
node --test                     # 假 daemon：帧顺序、错误码、发现优先级
$env:COREX_DAEMON = "...\corex-daemon.exe"
node --test test\daemon.smoke.js  # 真 daemon：发现 / 鉴权 / 目录 / 进度帧 / 收尾
```

假 daemon 那套钉的是**读法**；冒烟测试钉的是**两边真能说上话**。corex 改了线格式而没同步
客户端时，只有冒烟测试会红——发版前跑一次。
