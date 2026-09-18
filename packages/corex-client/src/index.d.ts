//! `corex-client` 的类型声明。
//!
//! 手写而不是从 TypeScript 生成：这个包**零依赖、零构建**（没有 tsc 这一步），
//! 宿主直接 `import` 的就是 `src/index.js` 本身。

/** daemon → 客户端的一帧错误。`code` 与 Rust 侧 `RpcError` 的辅助码对齐（400 / 401 / 403 / 404 / 500）。 */
export declare class RpcError extends Error {
  readonly code: number
  constructor(code: number, message: string)
}

/** Corex 的动态值：与 Rust 侧 `corex_core::Value` 对应的 JSON 形状。 */
export type CorexValue = unknown

/**
 * 进度帧的载荷，与 Rust 侧 `corex_ipc::ProgressEvent` 一一对应。
 *
 * `kind` 的三种取值与 `corex run --json-events` 是**同一套词汇**，不必记两套字段名。
 */
export interface ProgressEvent {
  kind: 'step_start' | 'step_progress' | 'step_end'
  /** 只有 `step_start` 有。 */
  seq?: number
  step: string
  action: string
  /** 只有 `step_progress` 有；`total` 未知时为 `null`。 */
  done?: number
  total?: number | null
  unit?: 'bytes' | 'items'
  /** 只有 `step_end` 有。 */
  took_ms?: number
  ok?: boolean
}

/** 目录里的一个动作；与 `corex actions --json` 的元素**逐字同形**。 */
export interface ActionEntry {
  id: string
  name: string
  description: string
  bucket: 'system' | 'network' | 'data' | 'ui' | 'logic' | 'plugin'
  params: Array<{
    name: string
    /** Corex 自己的类型标签：`str` / `file` / `map` / `any` … */
    ty: string
    required: boolean
    description?: string
    default?: CorexValue
  }>
  tags: string[]
  /** 动作声明的权限类别，如 `["filesystem"]`。 */
  permissions: string[]
  /** 可直接当 MCP 工具 `inputSchema` 用的 JSON Schema。 */
  input_schema: {
    type: 'object'
    properties: Record<string, CorexValue>
    required?: string[]
  }
}

/**
 * 动作目录文档；与 `corex actions --json`（以及 `list_actions` 的回话）**逐字同形**。
 *
 * `version` 是 daemon 的 corex 版本，用它判断手里的参数表要不要重拉。
 */
export interface Catalog {
  version: string
  count: number
  /** 筛过的组；不筛时为 `null`。 */
  bucket: string | null
  actions: ActionEntry[]
}

/** 磁盘上的端点记录（`<数据目录>/endpoint.json`）。 */
export interface EndpointRecord {
  version: number
  pid: number
  endpoint: string
  kind: 'pipe' | 'socket'
  /** 仅当 token 来自文件时才有。 */
  token_file?: string
}

/** [`discover`] 的结果。 */
export interface Discovered {
  dataDir: string
  endpoint: string
  /** 没找到就是 `undefined`（请求随后会以 401 现形）。 */
  token?: string
  kind: 'pipe' | 'socket' | null
  pid: number | null
  /** 端点是从哪来的：排错时第一个该看的东西。 */
  source: 'option' | 'record' | 'default'
}

/** 数据目录：显式 → `COREX_DATA_DIR` → 平台项目目录。 */
export declare function resolveDataDir(explicit?: string): string

/** 解析「连到哪、用什么 token」；顺序同 Rust 的 `find_endpoint`。 */
export declare function discover(options?: {
  dataDir?: string
  endpoint?: string
  token?: string
}): Discovered

/** [`spawnDaemon`] 的返回值。 */
export interface DaemonHandle {
  child: import('node:child_process').ChildProcess
  /** 等它自己退出，超时再强杀。正常的退出路径是 `client.close({ stopDaemon: true })`。 */
  stop(timeoutMs?: number): Promise<void>
  kill(): void
}

/** 拉起 `corex-daemon`（`daemonPath` 默认走 PATH，随应用分发时传 sidecar 绝对路径）。 */
export declare function spawnDaemon(options?: {
  daemonPath?: string
  args?: string[]
  env?: Record<string, string>
  /** 会设成子进程的 `COREX_DATA_DIR`；父子必须一致。 */
  dataDir?: string
  /** 会设成子进程的 `COREX_TOKEN`。 */
  token?: string
  cwd?: string
  /** daemon 的 stdout/stderr；不给就直接丢掉。 */
  onLog?: (text: string) => void
}): DaemonHandle

/** [`connect`] / [`CorexClient.connect`] 的选项。 */
export interface ClientOptions {
  dataDir?: string
  endpoint?: string
  token?: string
  /** 连不上就先拉起一个 daemon。 */
  spawn?: boolean
  spawnOptions?: Parameters<typeof spawnDaemon>[0]
  /** 建连上限（毫秒）。 */
  timeoutMs?: number
  /** `spawn: true` 时等就绪的上限（毫秒）。 */
  spawnTimeoutMs?: number
  onLog?: (text: string) => void
}

/** 与一个运行中的 daemon 的连接。 */
export declare class CorexClient {
  readonly endpoint: string
  readonly dataDir: string

  /** 连上 daemon；`spawn: true` 时连不上会先拉起一个。 */
  static connect(options?: ClientOptions): Promise<CorexClient>

  get isOpen(): boolean

  /** 这个客户端**自己拉起**的 daemon；连着别人的时是 `null`。 */
  readonly daemon: DaemonHandle | null

  /** daemon 退出（或连接断开）时的回调；返回退订函数。 */
  onClose(handler: () => void): () => void

  /** 探活。 */
  ping(options?: { timeoutMs?: number }): Promise<true>

  /** 动作目录文档（含 `version` 与 `actions`）。 */
  catalog(options?: { timeoutMs?: number }): Promise<Catalog>

  /** 动作清单（`catalog().actions`）：含参数表、权限与 `input_schema`。 */
  actions(options?: { timeoutMs?: number }): Promise<ActionEntry[]>

  /** 指令名；`dir` 是数据目录下的子目录，越界会被拒。 */
  directives(dir?: string, options?: { timeoutMs?: number }): Promise<string[]>

  /** 调用单个 Action。给了 `onProgress` 才会置 `stream: true`。 */
  invoke<T = CorexValue>(
    action: string,
    params?: Record<string, CorexValue>,
    options?: { onProgress?: (progress: ProgressEvent) => void; timeoutMs?: number }
  ): Promise<T>

  /** 执行指令。`file` 必须落在 daemon 的指令目录下。 */
  run<T = CorexValue>(
    name: string,
    options?: {
      input?: Record<string, CorexValue>
      file?: string
      onProgress?: (progress: ProgressEvent) => void
      timeoutMs?: number
    }
  ): Promise<T>

  /** 请 daemon 退出。 */
  shutdown(options?: { timeoutMs?: number }): Promise<void>

  /** 断开。自己拉起的 daemon 默认一并收掉。 */
  close(options?: { stopDaemon?: boolean }): Promise<void>
}

/** 等价于 [`CorexClient.connect`]。 */
export declare function connect(options?: ClientOptions): Promise<CorexClient>
