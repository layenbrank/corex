# Corex 架构与阶段实现

本文档描述 Corex 架构重构阶段 1–3 的实现细节。总览请参阅 [architecture-and-tauri-integration.md](./architecture-and-tauri-integration.md)。

---

## 阶段 1：统一 run 入口

### 目标

所有业务模块对外暴露统一的 `module::run(&Args) -> Result<()>` 签名，使 Daemon 可以按模块名字符串动态分发，无需经过 clap。

### 模块约定

每个业务模块结构：

```
module/
├── mod.rs       # pub use service::run; pub mod schema;
├── schema.rs    # Args 结构体（clap + serde）
└── service.rs   # pub fn run(&Args) -> Result<()>
```

已统一的模块：

| 模块 | 入口 | 实现文件 |
|------|------|----------|
| copy | `copy::run` | `copy/service.rs` |
| scrub | `scrub::run` | `scrub/service.rs` |
| shade | `shade::run` | `shade/service.rs` |
| compression | `compression::run` | `compression/service.rs` |
| generate | `generate::run` | `generate/service.rs` |
| exec | `exec::run` | `exec/service.rs` |
| bootstrap | `bootstrap::run` | `bootstrap/service.rs` |
| screenshot | `screenshot::run` | `screenshot/service.rs` |
| codec | `codec::run` | `codec/service.rs` |
| scan | `scan::run` | `scan/service.rs` |
| morph | `morph::run` | `morph/service.rs` |
| pipeline | `pipeline::run` | `pipeline/runner.rs` |
| schedule | `schedule::run` | `schedule/service.rs` |
| watch | `watch::run` | `watch/service.rs` |

screenshot 模块通过 `execute(&Args, Option<&[Monitor]>)` 支持 Daemon 传入缓存的显示器列表。

### CLI 分发

`corex-core/src/command/mod.rs` 定义 clap 命令树，`dispatch(args)` 路由到各模块：

```rust
pub fn dispatch(args: Args) -> Result<()> {
    match args.command {
        Commands::Copy(a) => copy::run(&a),
        Commands::Screenshot(a) => screenshot::run(&a),
        Commands::Pipeline(a) => pipeline::run(&a),
        Commands::Schedule(a) => schedule::run(&a),
        // ...
    }
}
```

### 模块 execute 契约

| 层 | 文件 | 职责 |
|----|------|------|
| `schema.rs` | Args（clap + serde） | 无业务逻辑 |
| `parse.rs` | 可选 | `${var.*}` / `${steps.*}` 占位符解析（`parse_args`） |
| `service.rs::execute` | 返回 `Output` | 纯业务，无 println |
| `service.rs::run` | CLI 包装 | execute + 人类输出 |
| `invoke/registry` | 薄路由 | `decode_json` → `parse_args` → `execute` → `InvokeResult` |

已实现 execute 的模块：compression、codec、scan、morph、screenshot、copy、scrub、shade、generate、bootstrap、exec。

### Invoke 模块 vs 非 Invoke 模块

**Invoke 模块**（可被 Pipeline step / IPC `invoke` 调用）：copy、scrub、shade、compression、generate、exec、screenshot、codec、scan、morph、bootstrap。

**非 Invoke 模块**（仅 CLI 入口，不在 `invoke/registry` 中注册）：

| 模块 | 入口 | 说明 |
|------|------|------|
| `pipeline` | `pipeline::run` | 编排器，内部调用 `invoke()` |
| `schedule` | `schedule::run` | 配置生成 / cron 调度，非 pipeline step |
| `watch` | `watch::run` | 文件变更监听 / debounce 后重跑 Pipeline，非 pipeline step |

screenshot 通过 `execute(args, cached_monitors)` 支持 Daemon 传入预热的显示器列表。

### Pipeline 与统一 Invoke

`corex-core/src/invoke/` 提供 `invoke(module, args, ctx)`，供 Pipeline orchestrator、IPC serve 共用（CLI 直接走各模块 `run()`）。`registry.rs` 按 module 名路由到各模块 `parse_args` + `execute()`。

Pipeline v3 由 `pipeline/orchestrator.rs` + `pipeline/graph.rs`（petgraph DAG）执行，支持 `depends_on`、`when`、`retry`。流式 generate Path 见 `pipeline/stream/path_stream.rs`。

配置见 [pipeline-v3.md](./pipeline-v3.md)；运行时见 [runtime.md](./runtime.md)。

### schedule 模块拆分

原 pipeline 中的 schedule 逻辑独立为 `schedule/` 模块：

- `schedule/schema.rs` — `Args::Run | Generate | Cron`
- `schedule/service.rs` — 交互式选择、cron 守护进程

### watch 模块

文件变更监听（Vite 风格 dev watch），debounce 后重跑整条 Pipeline：

- `watch/schema.rs` — `Args::Run { config, pipeline, debounce_ms, includes, excludes, immediate }`
- `watch/service.rs` — `resolve` / `run_loop` / `serve`；`notify-debouncer-full` 守护
- `pipeline/config.rs` — `PipelineConfig.watch: Option<WatchConfig>`

### trigger 与 guard 模块

`corex pipeline` 按 yaml 中的 `watch` / `schedule` 自动选择运行模式：

- `pipeline/trigger.rs` — `run_mode`、`check`、`serve_dual`；分发 Once / Watch / Cron / Dual
- `pipeline/guard.rs` — `RunningSet` 共享锁；`spawn`（watch 异步）、`run_sync`（cron 同步）

| yaml 配置 | `corex pipeline` | `corex pipeline --once` |
|-----------|------------------|-------------------------|
| 无 watch/schedule | 单次执行 | 单次执行 |
| 仅 watch | watch 守护 + `--immediate` | 单次执行 |
| 仅 schedule | schedule 守护 | 单次执行 |
| watch + schedule | 并行守护（共享锁） | 单次执行 |

```yaml
watch:
  paths: ['${var.base}/src']
  includes: []
  excludes: ['**/node_modules/**', '**/.git/**']
  debounce_ms: 300
```

glob 过滤工具：`utils/filter.rs`（原 `ignore.rs`，与 copy/generate 的 includes/excludes 命名一致）。

---

## 阶段 2：Cargo Feature 模块化

### Feature 依赖树

```
default = ["all"]
all = ["command"]
command = [cli, copy, scrub, shade, compression, generate, bootstrap, screenshot, codec, scan, morph, pipeline, schedule, watch]
cli = [dep:clap]
daemon = [cli, copy, scrub, shade, compression, generate, bootstrap, screenshot, codec, scan, morph]
serve = [daemon]
pipeline = [regex, serde_yml, dialoguer, crossterm, tokio, dirs, tasks]
schedule = [pipeline, cron, chrono]
watch = [pipeline, notify-fs, notify-debouncer-full, chrono, glob]
tasks = [codec, scan, morph, copy, scrub, shade, compression, generate, bootstrap, screenshot]
```

各模块 feature 还引入工具性子 feature：

| 子 feature | 依赖 | 用途 |
|------------|------|------|
| glob | glob | 路径过滤 |
| notify | notify-rust | 桌面通知 |
| progress | indicatif | 进度条 |

### 语义说明

| Feature | 含义 |
|---------|------|
| `default` / `all` | 完整 CLI 体验 |
| `command` | clap + 全部业务 + pipeline + schedule + watch |
| `daemon` | 业务模块 + cli，**不含** pipeline / schedule / watch |
| `serve` | daemon + serve 模块（Named Pipe IPC） |
| `screenshot` | 仅 xcap + image |

### 三 Binary 配置

**corex**（`corex/Cargo.toml`）：

```toml
[dependencies]
cx = { path = "../corex-core", features = ["all"] }
```

**corex-serve**（`corex-serve/Cargo.toml`）：

```toml
[dependencies]
cx = { path = "../corex-core", default-features = false, features = ["serve"] }
```

**corex-capture**（`corex-capture/Cargo.toml`）：

```toml
[dependencies]
cx = { path = "../corex-core", default-features = false, features = ["screenshot"] }
clap = { workspace = true }
```

### tokio 裁剪

workspace `Cargo.toml`：

```toml
tokio = { version = "1.49.0", features = ["rt-multi-thread", "macros", "fs", "sync"] }
```

从 `full` 改为按需 feature，减小编译体积与依赖树。

### 构建命令

```powershell
# 完整 CLI
cargo build -p corex --release

# Daemon（Tauri sidecar）
cargo build -p corex-serve --release

# 轻量截图
cargo build -p corex-capture --release

# 按需编译库（仅 screenshot）
cargo build -p corex-core --no-default-features --features screenshot
```

---

## 阶段 3：Daemon + JSON IPC

### 模块结构

```
serve/
├── mod.rs          # 公开 API：run, request, shutdown
├── protocol.rs     # Request / Response / parse_request
├── dispatch.rs     # 按 module 名分发
├── state.rs        # DaemonState（Monitor 缓存）
└── pipe/
    ├── mod.rs      # 平台分发
    └── windows.rs  # Named Pipe 服务端与客户端
```

### 公开 API

**`serve/mod.rs`：**

```rust
pub struct ServeOptions {
    pub pipe_name: String,  // 默认 \\.\pipe\corex
}

pub fn run(options: ServeOptions) -> anyhow::Result<()>;
pub fn request(pipe_name: &str, module: &str, args: Value) -> anyhow::Result<Response>;
pub fn shutdown(pipe_name: &str) -> anyhow::Result<()>;
```

### Daemon 生命周期

```mermaid
stateDiagram-v2
    [*] --> Init: serve::run
    Init --> Listen: DaemonState.init
    Listen --> HandleClient: client connects
    HandleClient --> HandleClient: Invoke 写响应后继续读
    HandleClient --> Listen: disconnect_pipe_file
    HandleClient --> [*]: Shutdown
```

1. **启动**：`DaemonState::init()` 调用 `Monitor::all()` 并缓存
2. **监听**：`CreateNamedPipeW` 循环等待连接
3. **处理**：`handle_client` loop 读行 JSON → invoke 或 shutdown
4. **响应**：每行 Invoke 写入 JSON + `\n`；连接结束才 `disconnect_pipe_file`
5. **退出**：收到 Shutdown（无响应）或 Ctrl+C

### Monitor 缓存

**`state.rs`：**

```rust
pub struct DaemonState {
    pub monitors: Option<Vec<Monitor>>,
}

impl DaemonState {
    pub fn init() -> anyhow::Result<Self> {
        let monitors = Monitor::all()?;
        // ...
        Ok(Self { monitors: Some(monitors) })
    }
}
```

**`screenshot/service.rs`：**

```rust
pub fn capture(args: &Args, cached_monitors: Option<&[Monitor]>) -> Result<PathBuf> {
    let monitors = match cached_monitors {
        Some(m) => m,
        None => { /* Monitor::all() 每次 */ }
    };
    // capture_image → save PNG
}
```

CLI 路径调用 `capture(args, None)`；Daemon 路径传入 `state.monitors.as_deref()`。

### dispatch 支持的 module

| module | 调用 | 返回 path | 返回 data |
|--------|------|-----------|-----------|
| screenshot | `screenshot::execute` | 写文件类操作 | monitors/windows 等 |
| copy | `copy::run(&args)` | `args.to` | — |
| scrub | `scrub::run(&args)` | `args.target` | — |
| shade | `shade::run(&args)` | `args.to` | — |
| compression | `compression::run(&args)` | zip/unzip 的 `to` | — |
| generate | `generate::execute` | Path 的 `to` | Uuid 无 path |
| exec | `exec::execute` | 脚本 JSON `path` | 脚本 JSON `data` |
| bootstrap | `bootstrap::run(&args)` | None | — |
| codec | `codec::execute` | 可选 output 文件 | text 等 |
| scan | `scan::execute` | None | OsContext JSON |
| morph | `morph::execute` | 写文件类操作 | meta/search/render 等 |

响应字段 `path` / `data` 说明见 [ipc-protocol.md](./ipc-protocol.md)。破坏性变更见 [breaking-changes.md](./breaking-changes.md)。

未知或未启用的 module 返回错误响应。

### Named Pipe 实现要点

**`pipe/windows.rs`：**

| 常量/函数 | 说明 |
|-----------|------|
| `MAX_LINE_BYTES` | 64KB 请求行上限 |
| `run_server` | 服务端主循环 |
| `handle_client` | 单连接多行协议（loop 读 Invoke，Shutdown 退出） |
| `send_request` | 库内 IPC 客户端（每请求新连接） |
| `send_shutdown` | 发送 shutdown 请求 |

非 Windows 平台：`pipe/mod.rs` 返回 `bail!("corex serve 当前仅支持 Windows Named Pipe")`。

协议详情见 [ipc-protocol.md](./ipc-protocol.md)。

---

## CLI 与 Daemon 使用

### corex（完整 CLI）

```powershell
corex --help
corex screenshot capture --to C:\Screenshots
corex copy --from ./src --to ./dist --excludes "node_modules"
corex pipeline --config pipelines.yaml
corex schedule run
corex watch run --immediate
```

完整命令说明见根目录 [README.md](../README.md)。

### corex-serve（Daemon）

```powershell
# 默认 Pipe
corex-serve

# 自定义 Pipe
corex-serve --pipe \\.\pipe\corex

# 开发模式
cargo run -p corex-serve
```

启动后会输出：

```
corex-serve: 已缓存 N 个显示器
corex-serve: 监听 Named Pipe \\.\pipe\corex（Ctrl+C 退出）
```

### corex-capture（轻量截图）

```powershell
corex-capture --to C:\Screenshots
cargo run -p corex-capture -- --to C:\Temp
```

无 Daemon、无 IPC，适合脚本一次性截图。

### 库内 IPC 调用

```rust
use cx::invoke::WireArgs;
use cx::serve;

let resp = serve::request(
    r"\\.\pipe\corex",
    "screenshot",
    WireArgs::action("capture", serde_json::json!({ "to": "C:/out" })),
)?;

if resp.ok {
    println!("path: {:?}", resp.path);
}

serve::shutdown(r"\\.\pipe\corex")?;
```

示例程序：`corex-core/examples/ipc.rs`。

---

## 与 Tauri 的关系

Tauri **不**依赖 `corex-core` crate，而是通过 Named Pipe 与 `corex-serve` 通信。Tauri 侧 `corex_ipc.rs` 独立实现 Pipe 客户端（约 200 行，仅 std + serde + windows）。

接入指南：[tauri-integration.md](./tauri-integration.md)。

---

## 开发注意事项

### 新增 Daemon 模块

1. 确保模块有 `run(&Args)` 且 `Args` 实现 `Deserialize`
2. 在 `dispatch.rs` 添加 match 分支
3. 若需返回 path，在 `DispatchResult` 中设置
4. 确保 `daemon` feature 包含该模块

### 测试 serve 协议

```powershell
cargo test -p corex-core --features serve protocol::
```

单元测试位于 `serve/protocol.rs`（parse_request 六种场景）。

### Feature 组合示例

```powershell
# 仅 copy + serve（自定义 binary 时）
cargo build -p corex-core --no-default-features --features "serve,copy"
```

注意：`serve` 已包含 `daemon` 默认模块集，单独追加 module feature 会扩展 dispatch 能力。
