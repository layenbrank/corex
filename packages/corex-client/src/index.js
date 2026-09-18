//! Corex daemon 的 NDJSON IPC 客户端（零依赖，ESM）。
//!
//! 与 Rust 侧的对应关系：
//!
//! | 这里 | Rust |
//! | ---- | ---- |
//! | [`discover`] | `corex_ipc::find_endpoint`（配置 → 记录 → 平台默认） |
//! | [`resolveDataDir`] | `corex_ipc::data_dir`（**少了「可写 exe 目录」那一档**，见下） |
//! | [`CorexClient`] | `corex_ipc::Transport` + `Request` / `Response` |
//!
//! 为什么要有这个包：宿主（Electron main、脚本、CLI 之外的任何进程）原先得自己拼出
//! `\\.\pipe\corex`、自己找 token、自己处理「进度帧排在终帧之前」这件事。三件都在这里
//! 做一次。
//!
//! ⚠️ **数据目录**：Rust 的 `data_dir()` 第一档是「二进制所在目录（可写时）」，而 JS 侧
//! 不知道 `corex.exe` 在哪，复刻不了。所以随宿主分发 corex 时**请显式设 `COREX_DATA_DIR`**
//! 或给 `connect({ dataDir })`；否则客户端只能按「`COREX_DATA_DIR` → 平台项目目录」去猜，
//! 而 daemon 可能正用着 exe 旁边那个目录。

import { spawn } from 'node:child_process'
import fs from 'node:fs'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'

/** 端点记录的文件名；与 Rust 侧 `endpoint::FILE` 一致。 */
const RECORD_FILE = 'endpoint.json'

/** 记录格式版本；不认识就当这份文件不存在。 */
const RECORD_FORMAT = 1

/** Windows 上的默认命名管道；与 Rust 侧 `ipc_endpoint()` 一致。 */
const DEFAULT_PIPE = '\\\\.\\pipe\\corex'

/** 轻量请求（探活、拉目录）的默认等待上限。 */
const LIGHT_TIMEOUT_MS = 10000

/** daemon → 客户端的一帧错误。`code` 与 `RpcError` 的四个辅助码对齐（400/401/403/404/500）。 */
export class RpcError extends Error {
  constructor(code, message) {
    super(message)
    this.name = 'RpcError'
    this.code = code
  }
}

/**
 * 数据目录：显式 → `COREX_DATA_DIR` → 平台项目目录。
 *
 * 对应 Rust 的 `data_dir()`，但**没有**「可写 exe 目录」那一档——那个目录客户端无从得知。
 * 随宿主分发 corex 时务必让两边看到同一个值，否则会以「连不上」的形式表现出来，很难查。
 */
export function resolveDataDir(explicit) {
  if (explicit) {
    return path.resolve(explicit)
  }
  const fromEnv = process.env.COREX_DATA_DIR
  return fromEnv ? path.resolve(fromEnv) : platformDataDir()
}

/** `directories::ProjectDirs::from("dev", "", "corex")` 的 JS 对应实现。 */
function platformDataDir() {
  if (process.platform === 'win32') {
    const roaming = process.env.APPDATA ?? path.join(os.homedir(), 'AppData', 'Roaming')
    // Windows 上 ProjectDirs 会在末尾补 `\data`，XDG 系不补。
    return path.join(roaming, 'corex', 'data')
  }
  if (process.platform === 'darwin') {
    return path.join(os.homedir(), 'Library', 'Application Support', 'corex')
  }
  // `directories` 只认绝对路径的 XDG_DATA_HOME，相对值一律回落到默认。
  const xdg = process.env.XDG_DATA_HOME
  const root = xdg && path.isAbsolute(xdg) ? xdg : path.join(os.homedir(), '.local', 'share')
  return path.join(root, 'corex')
}

/** 平台的默认端点（没有记录、也没有显式配置时）。 */
function defaultEndpoint(dataDir) {
  return process.platform === 'win32' ? DEFAULT_PIPE : path.join(dataDir, 'corex.sock')
}

/**
 * 读 daemon 写下的端点记录。缺失、JSON 损坏或版本不认识都是 `null`。
 *
 * 读不到就该退回平台默认，而不是让连接起不来——这正是 Rust 侧 `discover` 的取舍。
 */
function readRecord(dataDir) {
  let text
  try {
    text = fs.readFileSync(path.join(dataDir, RECORD_FILE), 'utf8')
  } catch {
    return null
  }
  let record
  try {
    record = JSON.parse(text)
  } catch {
    return null
  }
  if (!record || record.version !== RECORD_FORMAT || typeof record.endpoint !== 'string') {
    return null
  }
  return record
}

/** 读 token 文件；读不到返回 `undefined`（让请求以 401 现形，而不是在这里抛）。 */
function readTokenFile(file) {
  if (!file) {
    return undefined
  }
  try {
    const token = fs.readFileSync(file, 'utf8').trim()
    return token || undefined
  } catch {
    return undefined
  }
}

/**
 * 解析出「连到哪、用什么 token」，不做任何 IO 之外的副作用，便于把结果打进日志。
 *
 * 顺序与 Rust 的 `find_endpoint` 一致：**显式配置 → 端点记录 → 平台默认**。
 * 显式配置排最前是有意的——命令行/选项里写死的东西必须赢过发现。
 *
 * token 的顺序与 Rust 的 `find_token` 一致：`token` 选项 → `COREX_TOKEN` → 记录里的
 * `token_file` → `<数据目录>/token`。**记录在但没有 `token_file`** 时不去读那个文件：
 * daemon 的 token 来自 `COREX_TOKEN` 或配置（那两处属于调用方，不会被写进记录），
 * 此时 `<数据目录>/token` 是**上一个** daemon 留下的东西。
 *
 * ⚠️ token 来自**配置文件**（`[daemon].token`）时这里看不到，需要 `token` 显式传入：
 * Rust 侧能读 TOML，这里刻意不引 TOML 解析器。
 */
export function discover({ dataDir, endpoint, token } = {}) {
  const resolved = resolveDataDir(dataDir)
  const record = readRecord(resolved)
  const tokenFile = record ? record.token_file : path.join(resolved, 'token')
  return {
    dataDir: resolved,
    endpoint: endpoint ?? record?.endpoint ?? defaultEndpoint(resolved),
    token: token ?? process.env.COREX_TOKEN ?? readTokenFile(tokenFile),
    kind: record?.kind ?? null,
    pid: record?.pid ?? null,
    /** 端点是从哪来的：`option` / `record` / `default`。排错时第一个该看的东西。 */
    source: endpoint ? 'option' : record ? 'record' : 'default'
  }
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** 等子进程自己退出；超时返回 `false`（调用方再决定要不要强杀）。 */
function waitExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve(true)
  }
  return new Promise((resolve) => {
    let timer
    const onExit = () => {
      clearTimeout(timer)
      resolve(true)
    }
    timer = setTimeout(() => {
      child.off('exit', onExit)
      resolve(false)
    }, timeoutMs)
    child.once('exit', onExit)
  })
}

/**
 * 拉起 `corex-daemon`。
 *
 * 宿主应把数据目录与 token 一起交给子进程（这个函数会照 `options` 设好
 * `COREX_DATA_DIR` / `COREX_TOKEN`），否则父子各看一份配置——那是最难查的一种“连不上”。
 *
 * `daemonPath` 默认走 PATH；随应用分发时传 sidecar 的绝对路径。
 */
export function spawnDaemon({
  daemonPath = 'corex-daemon',
  args = [],
  env = {},
  dataDir,
  token,
  cwd,
  onLog
} = {}) {
  const childEnv = { ...process.env, ...env }
  if (dataDir) {
    childEnv.COREX_DATA_DIR = dataDir
  }
  if (token) {
    childEnv.COREX_TOKEN = token
  }

  const child = spawn(daemonPath, args, {
    env: childEnv,
    cwd,
    windowsHide: true,
    // daemon 的日志流按设计就是 stdout；没人要就直接丢掉，免得它把宿主的管道顶满。
    stdio: onLog ? ['ignore', 'pipe', 'pipe'] : 'ignore'
  })
  if (onLog) {
    // 交给 Node 的 `StringDecoder`（`setEncoding` 内部就是它）：一条日志被读块从
    // 汉字中间切开时，它会把半个字符留到下一块，而不是就地变成 U+FFFD。
    child.stdout.setEncoding('utf8')
    child.stderr.setEncoding('utf8')
    child.stdout.on('data', (chunk) => onLog(chunk))
    child.stderr.on('data', (chunk) => onLog(chunk))
  }

  return {
    child,
    /**
     * 请它退出的兜底：先等一会儿（正常的路径是连接上发 `shutdown`，见
     * `client.close({ stopDaemon: true })`），超时才强杀。
     */
    async stop(timeoutMs = 3000) {
      if (await waitExit(child, timeoutMs)) {
        return
      }
      child.kill()
    },
    kill() {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill()
      }
    }
  }
}

/** 与一个运行中的 daemon 的连接。用 [`connect`] 创建。 */
export class CorexClient {
  #socket = null
  #token
  #daemon
  #pending = new Map()
  #nextId = 1
  #buffer = ''
  #closed = false
  #closeHandlers = new Set()

  constructor({ endpoint, token, dataDir, daemon = null }) {
    this.endpoint = endpoint
    this.dataDir = dataDir
    this.#token = token
    this.#daemon = daemon
  }

  /**
   * 连上 daemon。
   *
   * - 默认连「已经跑着的那个」：`discover` 的优先级 + 一次就失败。
   * - `spawn: true` 时连不上就**先拉起一个**再连，这是宿主启动时的常规做法
   *   （Electron main 启动时 daemon 多半还没起）。
   *
   * 返回的客户端记住了“是自己起的还是连别人的”，`close()` 据此决定收不收掉它。
   */
  static async connect(options = {}) {
    const requested = discover(options)
    const dialTimeout = options.timeoutMs ?? 3000

    try {
      return await CorexClient.#join(requested, dialTimeout, null)
    } catch (err) {
      if (!options.spawn) {
        throw err
      }
    }

    const daemon = spawnDaemon({
      onLog: options.onLog,
      ...options.spawnOptions,
      // 数据目录必须两边一致，否则会以「连不上」的形式表现出来；token 由 daemon
      // 自己解析/创建，我们把显式给过的那个传下去就够了。
      dataDir: requested.dataDir,
      token: options.spawnOptions?.token ?? options.token
    })

    const deadline = Date.now() + (options.spawnTimeoutMs ?? 10000)
    let lastError = new Error('daemon 未就绪')
    for (;;) {
      if (daemon.child.exitCode !== null) {
        throw new Error(`corex-daemon 启动即退出（码 ${daemon.child.exitCode}）`)
      }
      // **每轮重新解析**：daemon 现在写下记录了（端点以它为准，它可能读到了另一份配置），
      // `<数据目录>/token` 也可能是它刚创建出来的——首次解析时那个文件还不存在。
      try {
        return await CorexClient.#join(discover(options), 500, daemon)
      } catch (err) {
        lastError = err
      }
      if (Date.now() > deadline) {
        await daemon.stop(0)
        throw new Error(`corex-daemon 未在超时内就绪（${lastError.message}）`)
      }
      await sleep(50)
    }
  }

  /** 建一条连接并装配好客户端。 */
  static async #join(settings, timeoutMs, daemon) {
    const socket = await CorexClient.#open(settings, timeoutMs)
    const client = new CorexClient({ ...settings, daemon })
    client.#attach(socket)
    return client
  }

  /** 底层流连上了没有。 */
  get isOpen() {
    return this.#socket !== null && !this.#closed
  }

  /**
   * 这个客户端**自己拉起**的 daemon 句柄；连着别人的 daemon 时是 `null`。
   *
   * 暴露出来是为了让宿主能监控它（看 pid、等退出）或者在 `close()` 之外兜底强杀。
   */
  get daemon() {
    return this.#daemon
  }

  /** daemon 退出（或连接断开）时的回调；返回退订函数。 */
  onClose(handler) {
    this.#closeHandlers.add(handler)
    return () => this.#closeHandlers.delete(handler)
  }

  /** 建连。失败时抛出的错误里带得上端点，因为“连不上”九成是端点不对。 */
  static async #open(settings, timeoutMs) {
    const socket = await new Promise((resolve, reject) => {
      const attempt = net.connect({ path: settings.endpoint })
      const timer = setTimeout(() => {
        attempt.destroy()
        reject(new Error(`连接 ${settings.endpoint} 超时（${timeoutMs}ms）`))
      }, timeoutMs)
      attempt.once('connect', () => {
        clearTimeout(timer)
        resolve(attempt)
      })
      attempt.once('error', (err) => {
        clearTimeout(timer)
        attempt.destroy()
        reject(new Error(`连接 ${settings.endpoint} 失败: ${err.message}`))
      })
    })
    return socket
  }

  #attach(socket) {
    this.#socket = socket
    // 同上：**必须**按流解码，不能对每块调 `chunk.toString()`。一个汉字被读块从
    // 中间切开时，后者会把它变成 U+FFFD —— 而 JSON 仍然合法，于是坏掉的是内容
    // 而不是解析（动作名与说明全是中文，`list_actions` 一次推 ~103 KB，切在
    // 多字节中间是常态）。
    socket.setEncoding('utf8')
    socket.on('data', (chunk) => this.#receive(chunk))
    socket.on('error', (err) => this.#failAll(err))
    socket.on('close', () => {
      this.#closed = true
      this.#failAll(new Error('与 daemon 的连接已断开'))
      for (const handler of this.#closeHandlers) {
        handler()
      }
    })
  }

  /**
   * 逐行拆帧。
   *
   * **一定不能把第一个帧当成回答**：置了 `stream` 的请求会先收到零个或多个
   * `event` 帧，终帧（`ok` / `error` / `pong` / `bye`）排在最后。按 `id` 归位是唯一
   * 正确的读法——一条连接上可以有别的请求的帧在飞。
   */
  #receive(text) {
    this.#buffer += text
    for (;;) {
      const end = this.#buffer.indexOf('\n')
      if (end < 0) {
        return
      }
      const line = this.#buffer.slice(0, end)
      this.#buffer = this.#buffer.slice(end + 1)
      if (line.trim()) {
        this.#dispatch(line)
      }
    }
  }

  #dispatch(line) {
    let message
    try {
      message = JSON.parse(line)
    } catch (err) {
      this.#failAll(new Error(`daemon 回话不是 JSON: ${line}`))
      return
    }
    const pending = this.#pending.get(message.id)
    if (!pending) {
      // 不属于任何在途请求：可能是超时后被丢掉的那条的回话，忽略而不是炸掉整条连接。
      return
    }
    if (message.type === 'event') {
      pending.onProgress?.(message.progress)
      return
    }
    this.#pending.delete(message.id)
    if (message.type === 'error') {
      pending.reject(new RpcError(message.error?.code ?? 0, message.error?.message ?? '未知错误'))
    } else {
      pending.resolve(message)
    }
  }

  #failAll(err) {
    const pending = [...this.#pending.values()]
    this.#pending.clear()
    for (const entry of pending) {
      entry.reject(err)
    }
  }

  #request(request, { onProgress, stream = false, timeoutMs = 0 } = {}) {
    if (!this.isOpen) {
      return Promise.reject(new Error('客户端未连接或已关闭'))
    }
    const id = this.#nextId++
    const payload = { ...request, id }
    if (this.#token) {
      payload.auth_token = this.#token
    }
    // 只要不给回调就不要帧：不置 `stream` 的请求一帧都不会收到，daemon 那边也省掉观察者。
    if (stream) {
      payload.stream = true
    }

    return new Promise((resolve, reject) => {
      // **默认不限时**：一条指令跑几分钟是正常的，给它卡一个上限只会把正常工作判成失败
      // （连接真的断了时 `close` / `error` 会把它拦下）。需要上限的调用方自己给。
      const timer =
        timeoutMs > 0
          ? setTimeout(() => {
              this.#pending.delete(id)
              reject(new Error(`请求 ${request.type} 超时（${timeoutMs}ms）`))
            }, timeoutMs)
          : null
      timer?.unref?.()
      this.#pending.set(id, {
        resolve: (message) => {
          clearTimeout(timer)
          resolve(message)
        },
        reject: (err) => {
          clearTimeout(timer)
          reject(err)
        },
        onProgress
      })
      this.#socket.write(`${JSON.stringify(payload)}\n`, (err) => {
        if (!err) {
          return
        }
        this.#pending.delete(id)
        clearTimeout(timer)
        reject(err)
      })
    })
  }

  /** 探活。`corex daemon status` 用的是同一个请求。 */
  async ping({ timeoutMs = LIGHT_TIMEOUT_MS } = {}) {
    await this.#request({ type: 'ping' }, { timeoutMs })
    return true
  }

  /**
   * 动作目录：与 `corex actions --json` 的**同一份文档**
   * （`{ version, count, bucket, actions }`）。
   *
   * `version` 是 daemon 的 corex 版本，宿主据此判断手里的参数表要不要重拉。
   */
  async catalog({ timeoutMs = LIGHT_TIMEOUT_MS } = {}) {
    const response = await this.#request({ type: 'list_actions' }, { timeoutMs })
    return response.data ?? { version: null, count: 0, actions: [] }
  }

  /**
   * 动作清单（就是 [`catalog`](#catalog) 里的 `actions` 数组）。
   *
   * 每个元素含 `id` / `name` / `description` / `bucket` / `params` / `permissions` /
   * `input_schema`——够 agent 直接拼工具清单。
   */
  async actions({ timeoutMs = LIGHT_TIMEOUT_MS } = {}) {
    const doc = await this.catalog({ timeoutMs })
    return doc.actions ?? []
  }

  /** 指令名。`dir` 是数据目录下的子目录（**路径沙箱**：越界会被拒）。 */
  async directives(dir, { timeoutMs = LIGHT_TIMEOUT_MS } = {}) {
    const request = { type: 'list_directives' }
    if (dir) {
      request.dir = dir
    }
    const response = await this.#request(request, { timeoutMs })
    return response.data ?? []
  }

  /**
   * 调用单个 Action。
   *
   * 给了 `onProgress` 就置 `stream: true`，`event` 帧按到达顺序回调；不给则一个字节的
   * 进度都不会传。
   */
  async invoke(action, params = {}, { onProgress, timeoutMs } = {}) {
    const response = await this.#request(
      { type: 'invoke', action, params },
      { onProgress, stream: Boolean(onProgress), timeoutMs }
    )
    return response.data
  }

  /**
   * 执行指令。
   *
   * - `input`：`{key: value}` 映射，对应 CLI 的 `-i key=value`
   * - `file`：指令文件路径；**必须落在 daemon 的指令目录下**（同样受沙箱约束）
   */
  async run(name, { input, file, onProgress, timeoutMs } = {}) {
    const request = { type: 'run_directive', name, input: input ?? {} }
    if (file) {
      request.path = file
    }
    const response = await this.#request(request, {
      onProgress,
      stream: Boolean(onProgress),
      timeoutMs
    })
    return response.data
  }

  /** 请 daemon 退出。它回 `bye` 后进程会结束（连接随之断掉）。 */
  async shutdown({ timeoutMs } = {}) {
    await this.#request({ type: 'shutdown' }, { timeoutMs })
  }

  /**
   * 断开。
   *
   * `stopDaemon` 默认取决于“这个客户端是不是自己拉起了 daemon”：自己起的就顺手请它退出
   * （谁起的谁收），连着别人的就只断开。`stopDaemon: true` 会在礼貌退出没等到时强杀兜底。
   */
  async close({ stopDaemon = this.#daemon !== null } = {}) {
    if (this.#closed) {
      return
    }
    if (stopDaemon && this.isOpen) {
      try {
        await this.shutdown()
      } catch {
        // 已经不在的 daemon 不值得再报一次；下面还有强杀兜底。
      }
    }
    this.#closed = true
    this.#socket?.destroy()
    this.#socket = null
    this.#failAll(new Error('客户端已关闭'))
    await this.#daemon?.stop()
  }
}

/**
 * 连上 daemon；等价于 [`CorexClient.connect`]。
 *
 * 选项：
 *
 * - `dataDir` / `endpoint` / `token`：显式覆盖发现结果
 * - `spawn`：连不上就先拉起一个 daemon（`spawnOptions` 透传给 [`spawnDaemon`]）
 * - `timeoutMs` / `spawnTimeoutMs`：建连与等就绪的上限
 * - `onLog`：daemon 的 stdout/stderr 回调
 */
export function connect(options = {}) {
  return CorexClient.connect(options)
}
