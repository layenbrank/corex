//! Corex 守护进程 —— 读配置、注册内置动作、服务 IPC。

use anyhow::{Context, Result, bail};
use clap::Parser;
use corex_core::{
    ActionError, ActionStore, DaemonConfig, EngineError, ExecutionContext, LoggingConfig, Mark,
    Observer, PermissionKind, PermissionSet, RuntimeConfig, Spot, Stream, Value,
    check_runtime_allowed,
};
use corex_engine::{
    AuditEntry, Directive, DirectiveHistory, ExecutionAudit, ExecutionHistory, HistoryEntry,
    Pipeline, required_permissions, validate_allowed, validate_registered,
};
use corex_ipc::protocol::{Request, Response, RpcError};
use corex_ipc::{FrameSink, Outlet, ProgressEvent, config_paths, data_dir, serve_ipc_ready};
use corex_registry::ActionRegistry;
use fs2::FileExt;
use rand::RngExt;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Semaphore;
#[cfg(unix)]
use tracing::error;
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "corex-daemon", version, about = "Corex background daemon")]
struct Args {
    /// 覆盖 IPC 端点（Unix socket 路径，或 Windows 命名管道，如 \\.\pipe\corex）
    #[arg(long, alias = "pipe")]
    socket: Option<PathBuf>,

    /// 覆盖指令目录
    #[arg(long)]
    directives: Option<PathBuf>,

    /// 配置文件（toml）
    #[arg(long)]
    config: Option<PathBuf>,
}

struct DaemonState {
    registry: Arc<ActionRegistry>,
    config: RuntimeConfig,
    directives_dir: PathBuf,
    history: Option<ExecutionHistory>,
    audit: Option<ExecutionAudit>,
    auth_token: String,
    shutdown: AtomicBool,
    /// 同时执行的请求数上限；见 `[daemon].max_jobs`。
    jobs: Arc<Semaphore>,
    /// `ui.*` 与 `capture.*` 共享屏幕 / 输入设备，不能同时执行。
    interactive: Arc<Semaphore>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let data = data_dir()?;
    // 配置文存在却解析失败是致命错误：回退到默认值会静默丢掉 `strict_permissions`。
    let paths: Vec<PathBuf> = args
        .config
        .as_deref()
        .map(|p| vec![p.to_path_buf()])
        .unwrap_or_else(config_paths);
    let resolved = corex_core::config::read(&paths)?;
    let config = resolved.config;
    init_tracing(&config.logging);
    if let Some(source) = &resolved.source {
        info!(path = %source.display(), "配置已加载");
    }
    for issue in &resolved.warnings {
        warn!(key = issue.key, "{}", issue.message);
    }

    let endpoint = resolve_endpoint(args.socket, &config.daemon, &data)?;
    let lock_path = resolve_lock_path(&config.daemon, &data);
    // 宿主看到的第一眼就是这个列表：默认目录为空时放入起步指令。`--directives` 是调用方
    // 自己指的目录，不碰。
    let directives_dir = match args.directives {
        Some(dir) => {
            std::fs::create_dir_all(&dir)?;
            dir
        }
        None => {
            let dir = data.join("directives");
            if let Err(e) = corex_engine::starter::seed(&dir) {
                warn!(error = %e, dir = %dir.display(), "起步指令写入失败");
            }
            dir
        }
    };

    let auth = resolve_auth_token(&data, &config.daemon)?;

    let _lock = acquire_singleton(&lock_path)?;

    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    registry.remove_disabled(&config.plugins);
    info!(actions = registry.len(), "内置动作已注册");

    {
        let plugin_dir = if config.plugins.plugin_dir.is_absolute() {
            config.plugins.plugin_dir.clone()
        } else {
            data.join(&config.plugins.plugin_dir)
        };
        match corex_registry::discovery::discover(&plugin_dir, &mut registry) {
            Ok(found) => info!(count = found.len(), "插件发现完成"),
            Err(e) => warn!(error = %e, "插件发现失败"),
        }
    }

    let history = open_history(&data, &config)?;
    let audit = ExecutionAudit::under_data_dir(&data).ok();

    // `max_jobs = 0` 表示不限：拿信号量的最大许可数当“无限”。
    let jobs = Arc::new(Semaphore::new(match config.daemon.max_jobs {
        0 => Semaphore::MAX_PERMITS,
        n => n,
    }));
    // UI 与 capture 是一类共享资源；其它动作仍只受 max_jobs 限制，可以并行。
    let interactive = Arc::new(Semaphore::new(1));

    let state = Arc::new(DaemonState {
        registry: Arc::new(registry),
        config,
        directives_dir,
        history,
        audit,
        auth_token: auth.token,
        shutdown: AtomicBool::new(false),
        jobs,
        interactive,
    });

    // 信号处理
    let flag = Arc::clone(&state);
    tokio::spawn(async move {
        shutdown_signal().await;
        info!("收到停止信号");
        flag.shutdown.store(true, Ordering::SeqCst);
        // 尽力而为：删掉 socket，使服务循环报错退出 / 客户端快速失败。
    });

    info!(endpoint = %endpoint.display(), "corex-daemon 启动");

    // 记录在**端点就绪之后**才写（`serve_ipc_ready` 的回调）：这样「有记录」就等于
    // 「端点正听着」，而不是「有人正打算监听」。守卫借用单例锁，把「先删记录、再放锁」
    // 变成编译期约束——反过来会让旧进程删掉新 daemon 刚写下的那份记录。
    let record = PublishedRecord::of(&data, &_lock);
    let state_serve = Arc::clone(&state);
    let result = serve_ipc_ready(
        &endpoint,
        || record.write(&endpoint, auth.file.clone()),
        move |req, outlet| {
            let state = Arc::clone(&state_serve);
            async move { handle_request(&state, req, outlet).await }
        },
    )
    .await;

    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&endpoint);
    }
    info!("corex-daemon 已退出");
    result.context("IPC 服务异常")?;
    Ok(())
}

/// 端点记录的守卫：`write` 写下记录，`Drop` 时删掉。
///
/// 用守卫而不是在末尾补一行 `retract`：服务异常退出、将来有人在中间加个 `?`，都不该
/// 留下一份指向死端点的记录让下一个连接方白跑一趟。
///
/// 它**借用**单例锁（而不是各自独立地声明），于是「先删记录、再放锁」成了编译期约束：
/// 反过来会让下一个 daemon 读到一份指向**上一个**进程端点的记录，而它自己刚写下的那份
/// 又被旧进程删掉。
struct PublishedRecord<'a> {
    data: PathBuf,
    _lock: &'a File,
}

impl<'a> PublishedRecord<'a> {
    fn of(data: &Path, lock: &'a File) -> Self {
        Self {
            data: data.to_path_buf(),
            _lock: lock,
        }
    }

    /// 写下记录（在端点 bind 之后调，见 [`corex_ipc::serve_ipc_ready`]）。
    ///
    /// 写不进去只警告：发现文件是便利设施，缺了连接方仍能退回平台默认端点，
    /// 而因为一个杂项文件写不下就拒绝启动，是把便利设施当成了必需品。
    fn write(&self, endpoint: &Path, token_file: Option<PathBuf>) {
        let record = corex_ipc::endpoint::Record::new(endpoint, token_file);
        if let Err(e) = corex_ipc::endpoint::publish(&self.data, &record) {
            warn!(error = %e, "端点记录写不下，连接方得自己解析端点");
        }
    }
}

impl Drop for PublishedRecord<'_> {
    fn drop(&mut self) {
        corex_ipc::endpoint::retract(&self.data);
    }
}

/// 把流水线进度推给正在等这条请求的连接。
///
/// 一帧都不 `await`：`frame` 会被 `parallel` 分支并发调用，一条渲染慢（或干脆
/// 不再读）的客户端不该让 daemon 的流水线慢下来——`Outlet` 队列满即丢帧。
#[derive(Debug)]
struct StreamObserver {
    outlet: Outlet,
    seq: AtomicU64,
}

impl StreamObserver {
    fn new(outlet: Outlet) -> Self {
        Self {
            outlet,
            seq: AtomicU64::new(0),
        }
    }

    /// 组装一帧交给 [`Outlet`]。心跳不走这里——它不对应任何一步，用不上 `seq`。
    fn emit(&self, event: ProgressEvent) {
        self.outlet.frame(&event);
    }

    /// 序号就是「第几步」：一步只在 `begin` 占一个号，同一步里的其余帧跟着它走。
    fn next_step(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed) + 1
    }
}

impl Observer for StreamObserver {
    fn begin(&self, at: Spot<'_>) {
        self.emit(ProgressEvent::StepStart {
            seq: self.next_step(),
            step: at.id.to_string(),
            action: at.action.to_string(),
        });
    }

    fn chunk(&self, at: Spot<'_>, mark: Mark) {
        self.emit(ProgressEvent::StepProgress {
            step: at.id.to_string(),
            action: at.action.to_string(),
            done: mark.done,
            total: mark.total,
            unit: mark.unit,
        });
    }

    fn output(&self, at: Spot<'_>, stream: Stream, text: &str) {
        self.emit(ProgressEvent::StepOutput {
            step: at.id.to_string(),
            action: at.action.to_string(),
            stream,
            text: text.to_string(),
        });
    }

    fn end(&self, at: Spot<'_>, took: Duration, ok: bool) {
        self.emit(ProgressEvent::StepEnd {
            step: at.id.to_string(),
            action: at.action.to_string(),
            took_ms: took.as_millis() as u64,
            ok,
        });
    }
}

/// 心跳间隔。够密——客户端的「静止期」时限（Studio 是 60s）能稳稳地把它当保活；
/// 又够疏——一帧百来字节，排一小时的队也攒不满带宽。
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);

/// 执行请求的保活心跳：从建立起每 [`HEARTBEAT_INTERVAL`] 推一帧，附带「在排队还是在跑」。
///
/// 排队是心跳唯一要说的那件事：排队期间流水线一帧不出，客户端分不清「在等」与「死了」，
/// 而一段几分钟的排队足够撞穿宿主的请求时限（超时被报成失败，请求其实还排在队列里，
/// 之后照样执行）。心跳把这个歧义按帧喂给客户端。
///
/// 只有流式请求有心跳：不置 `stream` 的客户端读一行就完，多推的帧会被它当成终帧。
/// 守卫持有停止通道的发送端，所以把它搬进 `RunDirective` / `Invoke` 分支后，
/// `tokio::spawn` 的心跳任务在请求结束（含客户端提前断开）时随之停表。
#[derive(Debug)]
struct Heartbeat {
    is_queued: Arc<AtomicBool>,
    _stop: tokio::sync::oneshot::Sender<()>,
}

impl Heartbeat {
    fn start(outlet: &Outlet, is_queued: bool) -> Self {
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let outlet = outlet.clone();
        let queued = Arc::new(AtomicBool::new(is_queued));
        let ticker = queued.clone();
        tokio::spawn(async move {
            let mut waited = Duration::ZERO;
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {}
                }
                waited += HEARTBEAT_INTERVAL;
                outlet.frame(&ProgressEvent::Heartbeat {
                    is_queued: ticker.load(Ordering::Relaxed),
                    waited_ms: waited.as_millis() as u64,
                });
            }
        });
        Self {
            is_queued: queued,
            _stop: stop,
        }
    }

    /// 排到执行名额了：之后的帧报「在跑」。
    fn mark_running(&self) {
        self.is_queued.store(false, Ordering::Relaxed);
    }
}

async fn handle_request(state: &DaemonState, req: Request, outlet: Outlet) -> Response {
    let id = req.id();
    if !token_matches(req.auth_token(), &state.auth_token) {
        return Response::error(id, RpcError::unauthorized("invalid or missing auth token"));
    }
    if state.shutdown.load(Ordering::SeqCst) {
        return Response::Bye { id };
    }

    // 只有显式置了 `stream` 的请求才建上报口：其余请求连一次额外写入也不会有。
    let observer = req
        .wants_stream()
        .then(|| Arc::new(StreamObserver::new(outlet.clone())) as Arc<dyn Observer>);
    // 心跳也要在这里就起：排队正是最需要它的时段，而 `acquire` 之后就没有「排队」可言了。
    let heartbeat = observer
        .is_some()
        .then(|| Heartbeat::start(&outlet, req.is_execution()));

    // 先把指令解析成要执行的模型：调度和实际执行必须看同一份内容，避免文件在
    // 两次读取之间被编辑后绕过 UI / capture 的资源门。
    let prepared_directive = match &req {
        Request::RunDirective { name, path, .. } => {
            match load_directive(state, name, path.as_deref()) {
                Ok(directive) => Some(directive),
                Err(e) => return Response::error(id, classify(&e)),
            }
        }
        _ => None,
    };
    let needs_interactive = match &req {
        Request::Invoke { action, .. } => needs_interactive(state.registry.permissions_of(action)),
        Request::RunDirective { .. } => prepared_directive
            .as_ref()
            .is_some_and(|directive| directive_needs_interactive(&state.registry, directive)),
        _ => false,
    };

    // 先占共享设备，再占全局执行名额。这样多个 UI 请求在设备队列里等待时，不会
    // 先吃满 `max_jobs`，把本来可以并行的非 UI 工作挡在门外。
    let _interactive_permit = if needs_interactive {
        match state.interactive.clone().acquire_owned().await {
            Ok(permit) => Some(permit),
            Err(_) => return Response::error(id, RpcError::internal("交互资源队列已关闭")),
        }
    } else {
        None
    };

    // **执行类**请求排这个队；控制类（探活、状态、列目录）不排——它们是宿主判断
    // “daemon 还活着吗”的手段，被一条几分钟的指令堵住才是最糟的。
    // 信号量在本进程里从不关闭（`Arc` 与 daemon 同寿），所以错误分支只是形式上的。
    let _job_permit = match &req {
        Request::RunDirective { .. } | Request::Invoke { .. } => {
            match state.jobs.clone().acquire_owned().await {
                Ok(permit) => Some(permit),
                Err(_) => return Response::error(id, RpcError::internal("执行队列已关闭")),
            }
        }
        _ => None,
    };
    if let Some(heartbeat) = &heartbeat {
        heartbeat.mark_running();
    }

    match req {
        Request::Ping { id, .. } => Response::Pong { id },
        Request::Shutdown { id, .. } => {
            state.shutdown.store(true, Ordering::SeqCst);
            Response::Bye { id }
        }
        Request::ListActions { id, .. } => {
            // 整个目录而不是一串 id：宿主与 agent 需要知道「怎么调」——参数类型、
            // 默认值与要声明的权限都在里面。形状就是 `corex actions --json` 那一份
            // `catalog::document`（含 `version`，宿主据此判断参数表要不要重拉）。
            Response::ok(
                id,
                Value::from_json(corex_registry::catalog::document(&state.registry, None)),
            )
        }
        Request::ListDirectives { id, dir, .. } => match list_directives(state, dir.as_deref()) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::ListRuns {
            id, name, limit, ..
        } => match list_runs(state, name.as_deref(), limit) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::ReadDirective { id, name, dir, .. } => {
            match read_directive(state, &name, dir.as_deref()) {
                Ok(data) => Response::ok(id, data),
                Err(e) => Response::error(id, e),
            }
        }
        Request::SaveDirective {
            id,
            name,
            dir,
            definition,
            ..
        } => match save_directive(state, &name, dir.as_deref(), definition) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::RunDirective { id, input, .. } => match run_directive(
            state,
            prepared_directive
                .as_ref()
                .expect("RunDirective 已在调度前解析"),
            input,
            observer.as_ref(),
        )
        .await
        {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::error(id, classify(&e)),
        },
        Request::Invoke {
            id, action, params, ..
        } => match invoke_action(state, &action, params, observer.as_ref()).await {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::error(id, classify(&e)),
        },
    }
}

/// 把一条失败映射成 IPC 的错误码。
///
/// 与 CLI 的 `ExitStatus::read` 同一个做法：遍历 `anyhow` 链逐个 downcast。**不问消息
/// 文本**——`contains("权限")` 会把「文件权限不足」这类运行期失败也报成 403；也不能单独
/// 用 `kind()`：`not_found` 在动作上指文件 / 窗口没了，在引擎上指指令不存在。
fn classify(err: &anyhow::Error) -> RpcError {
    for cause in err.chain() {
        if let Some(action) = cause.downcast_ref::<ActionError>() {
            return from_action(action);
        }
        if let Some(engine) = cause.downcast_ref::<EngineError>() {
            return match engine {
                EngineError::StepFailed { source, .. } => from_action(source),
                EngineError::Action(action) => from_action(action),
                other => from_engine(other),
            };
        }
    }
    RpcError::internal(err.to_string())
}

/// 动作侧：门禁拒绝是 403，参数写错是 400，其余（`io` / `timeout` / 运行期 `not_found` …）
/// 都是 500——动作跑了但没成功。
fn from_action(err: &ActionError) -> RpcError {
    let message = err.to_string();
    match err.kind().as_str() {
        "permission_denied" | "disabled" => RpcError::forbidden(message),
        "invalid_params" => RpcError::invalid(message),
        _ => RpcError::internal(message),
    }
}

/// 引擎侧：`not_found` 是用户点名的指令（404），解析 / 用法 / 配置类是调用方的问题（400）。
fn from_engine(err: &EngineError) -> RpcError {
    let message = err.to_string();
    match err.kind().as_str() {
        "not_found" => RpcError::not_found(message),
        "not_registered" | "parse" | "config" | "usage" => RpcError::invalid(message),
        _ => RpcError::internal(message),
    }
}

/// 指令目录下的一条指令。
///
/// `name` 是后续 `read_directive` / `save_directive` 要的键，`path` 供宿主显示或交给
/// 外部编辑器，`bucket` 供宿主分组。分类得先解析文件才知道，所以**解析不了的指令照样
/// 列出来**（编辑器要能打开它去修），只是 `summary` 为空。
#[derive(Serialize)]
struct DirectiveEntry {
    name: String,
    path: String,
    bucket: Option<String>,
    summary: Option<DirectiveSummary>,
    /// 最近一次执行（`[history]` 关掉、或这条从没跑过时没有）。
    ///
    /// 卡片的「上次执行时间 / 上次成功没」直接用它——宿主自己攒一份必然与引擎的账本对不上。
    #[serde(skip_serializing_if = "Option::is_none")]
    last_run: Option<DirectiveHistory>,
}

/// 一次运行历史的回话。
///
/// `is_history_enabled` 与 `entries` 都得有：**关掉历史**与**一条都没跑过**都是空表，
/// 但卡片上一个该说「没开历史」，另一个才说「从未运行」。
#[derive(Serialize)]
struct RunsReply {
    is_history_enabled: bool,
    entries: Vec<HistoryEntry>,
}

/// `list_runs` 不给 `limit` 时回多少条：够卡片列表与运行台头几屏用。
const DEFAULT_RUNS: usize = 50;

/// 卡片要用的元信息：宿主列目录时不该为了显示描述、步骤数再把每个文件读一遍。
#[derive(Serialize)]
struct DirectiveSummary {
    description: String,
    step_count: usize,
    input_count: usize,
    trigger_count: usize,
}

/// 一条指令的原文与模型。
///
/// 两个都给：宿主展示与「保留自己没改的字段」要用原文，编辑要用模型。让宿主自己
/// 解析一遍 YAML 的话，编辑器里的形状迟早会和真跑的那份不一样。
#[derive(Serialize)]
struct DirectiveDocument {
    name: String,
    path: String,
    text: String,
    definition: Directive,
}

fn list_directives(state: &DaemonState, dir: Option<&str>) -> Result<Value, RpcError> {
    let base =
        resolve_dir(&state.directives_dir, dir).map_err(|e| RpcError::forbidden(e.to_string()))?;
    let entries = directive_entries(&base, state.history.as_ref())
        .map_err(|e| RpcError::internal(e.to_string()))?;
    as_data(&entries)
}

/// 最近的执行记录（新 → 旧），可按指令过滤。
///
/// 历史是引擎在跑完的当口自己写的；这里只是把它读出来——宿主存一份自己的「上次运行时间」
/// 就会与这份账本各说各话（换台机器、清过数据目录都会露馅）。
fn list_runs(
    state: &DaemonState,
    name: Option<&str>,
    limit: Option<usize>,
) -> Result<Value, RpcError> {
    let Some(history) = &state.history else {
        return as_data(&RunsReply {
            is_history_enabled: false,
            entries: Vec::new(),
        });
    };
    as_data(&RunsReply {
        is_history_enabled: true,
        entries: history.recent(name, limit.unwrap_or(DEFAULT_RUNS)),
    })
}

fn read_directive(state: &DaemonState, name: &str, dir: Option<&str>) -> Result<Value, RpcError> {
    let base =
        resolve_dir(&state.directives_dir, dir).map_err(|e| RpcError::forbidden(e.to_string()))?;
    let file = resolve_directive(&base, name).map_err(|e| from_engine(&e))?;
    let text = std::fs::read_to_string(&file).map_err(|e| RpcError::internal(e.to_string()))?;
    let definition = Directive::from_yaml_str(&text).map_err(|e| from_engine(&e))?;
    as_data(&DirectiveDocument {
        name: name.to_owned(),
        path: for_host(&file),
        text,
        definition,
    })
}

/// 保存一条指令，并把**规范化之后**的那份还给宿主。
///
/// 先过 `run` 走的那**两道门**再落盘：动作没注册是 400、权限声明不够是 403，两者
/// 写进去都跑不起来，不如当场拒掉——免得宿主存下一份注定失败的文件。
fn save_directive(
    state: &DaemonState,
    name: &str,
    dir: Option<&str>,
    definition: Value,
) -> Result<Value, RpcError> {
    let base =
        resolve_dir(&state.directives_dir, dir).map_err(|e| RpcError::forbidden(e.to_string()))?;
    let directive: Directive = serde_json::from_value(definition.to_json())
        .map_err(|e| RpcError::invalid(format!("指令定义不合法: {e}")))?;
    validate_registered(&*state.registry, &directive).map_err(|e| from_engine(&e))?;
    validate_allowed(&*state.registry, &directive).map_err(|e| from_action(&e))?;
    let text = directive.to_yaml_str().map_err(|e| from_engine(&e))?;
    let file = save_target(&base, name).map_err(|e| from_engine(&e))?;
    write_atomic(&file, &text).map_err(|e| RpcError::internal(e.to_string()))?;
    as_data(&DirectiveDocument {
        name: name.to_owned(),
        path: for_host(&file),
        text,
        definition: directive,
    })
}

/// 序列化成一条响应的 `data`。
fn as_data<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value)
        .map(Value::from_json)
        .map_err(|e| RpcError::internal(e.to_string()))
}

fn load_directive(state: &DaemonState, name: &str, path: Option<&str>) -> Result<Directive> {
    let file = if let Some(p) = path {
        confine_under(&state.directives_dir, Path::new(p))
            .with_context(|| format!("指令路径越界: {p}"))?
    } else {
        resolve_directive(&state.directives_dir, name)?
    };
    Ok(Directive::from_yaml_file(&file)?)
}

fn needs_interactive(permissions: PermissionSet) -> bool {
    permissions.contains(PermissionKind::Ui) || permissions.contains(PermissionKind::Capture)
}

fn directive_needs_interactive(registry: &ActionRegistry, directive: &Directive) -> bool {
    // Unknown actions will fail in the pipeline. Treat them as resource-using here so a
    // partially invalid directive cannot make a known UI/capture request overlap another one.
    validate_registered(registry, directive).is_err()
        || needs_interactive(required_permissions(registry, directive))
}

async fn run_directive(
    state: &DaemonState,
    directive: &Directive,
    input: std::collections::HashMap<String, Value>,
    observer: Option<&Arc<dyn Observer>>,
) -> Result<Value> {
    let ctx = ExecutionContext::new(state.config.clone()).with_input(input);
    let mut pipeline = Pipeline::new(state.registry.clone());
    if let Some(history) = &state.history {
        pipeline = pipeline.with_history(history.clone());
    }
    if let Some(audit) = &state.audit {
        pipeline = pipeline.with_audit(audit.clone());
    }
    if let Some(observer) = observer {
        pipeline = pipeline.with_observer(Arc::clone(observer));
    }
    Ok(pipeline.execute(directive, ctx).await?)
}

async fn invoke_action(
    state: &DaemonState,
    action_id: &str,
    params: Value,
    observer: Option<&Arc<dyn Observer>>,
) -> Result<Value> {
    check_invoke_allowed(&state.config, &*state.registry, action_id)?;
    let action = state
        .registry
        .get(action_id)
        .with_context(|| format!("动作未注册: {action_id}"))?;
    let at = Spot {
        id: "invoke",
        action: action_id,
    };
    let t0 = std::time::Instant::now();
    let mut ctx = ExecutionContext::new(state.config.clone());
    // 让动作的 `ctx.chunk()` 知道自己属于哪一步。指令路径上这是 `Pipeline` 的职责，
    // 而单动作直调绕过了它——不补这一步，动作上报的分块进度会全部落空。
    ctx.enter_step(at.id, at.action);
    if let Some(observer) = observer {
        ctx.observer = Some(Arc::clone(observer));
        observer.begin(at);
    }
    let outcome = async {
        action.validate(&params).await?;
        action.execute(params, &mut ctx).await
    }
    .await;
    ctx.leave_step();
    let duration_ms = t0.elapsed().as_millis() as u64;
    if let Some(observer) = observer {
        observer.end(at, t0.elapsed(), outcome.is_ok());
    }
    if let Some(audit) = &state.audit {
        let entry = AuditEntry::from_action(
            "invoke",
            "invoke",
            action_id,
            duration_ms,
            outcome.as_ref().map(|_| ()),
        );
        audit.record_best_effort(&entry);
    }
    Ok(outcome?)
}

/// 严格模式 + 配置层面的停用（与 `corex ui` 共用 [`check_runtime_allowed`]）。
/// 被禁用的动作也会通过 `remove_disabled` 从注册表里移除。
///
/// **保留 `ActionError` 的类型**：调用方要按错误种类回 IPC 码（见 [`classify`]），
/// 而 `anyhow!("{e}")` 会把它压成字符串，只剩 `contains` 可猜。
fn check_invoke_allowed(
    config: &RuntimeConfig,
    store: &dyn corex_core::ActionStore,
    action_id: &str,
) -> Result<(), ActionError> {
    check_runtime_allowed(config, store, action_id)
}

/// 按名称解析指令：只在 `dir` 下找 `{name}.yaml` / `{name}.yml`。
fn resolve_directive(dir: &Path, name: &str) -> Result<PathBuf, EngineError> {
    check_directive_name(name)?;
    let yaml = dir.join(format!("{name}.yaml"));
    let yml = dir.join(format!("{name}.yml"));
    if yaml.is_file() {
        return Ok(yaml);
    }
    if yml.is_file() {
        return Ok(yml);
    }
    Err(EngineError::DirectiveNotFound(name.to_owned()))
}

/// 保存时的目标文件：**写回 [`resolve_directive`] 会读到的那一个**，顺序必须一致。
///
/// 反过来（先看 `.yml`）会出现「改了没生效」：两份同名文件并存时执行的是 `.yaml`，
/// 保存却改进 `.yml`。已有的 `.yml` 仍写回 `.yml`，不另生 `.yaml` 副本。
fn save_target(dir: &Path, name: &str) -> Result<PathBuf, EngineError> {
    check_directive_name(name)?;
    let yaml = dir.join(format!("{name}.yaml"));
    if yaml.is_file() {
        return Ok(yaml);
    }
    let yml = dir.join(format!("{name}.yml"));
    if yml.is_file() {
        return Ok(yml);
    }
    Ok(yaml)
}

/// 指令名必须是裸名：`..` / 分隔符 / 盘符 / 绝对路径都会让读写跑到指令根之外。
///
/// 只判 [`Path::is_absolute`] 不够：Windows 上 `D:evil` 是**盘符相对**名，
/// `dir.join("D:evil.yaml")` 见到盘符 prefix 会把 base 整个丢掉（`Path::push` 的语义），
/// 落点是「D 盘的当前目录」——照样跑出指令根之外。所以要求整个名字**恰好是一个普通路径
/// 组件**（分隔符 / 盘符 / `.` / `..` 都不算）。另加一条 `\`：它在 Unix 上不是分隔符，
/// 但同一条指令名会跨平台落进 YAML 与 IPC 请求，不该只在 Windows 上被挡。
fn check_directive_name(name: &str) -> Result<(), EngineError> {
    let mut components = Path::new(name).components();
    let is_bare =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if !is_bare || name.contains("..") || name.contains('\\') {
        return Err(EngineError::Usage(format!("非法指令名: {name}")));
    }
    Ok(())
}

/// 先写同目录的临时文件再改名：**读到一个写了一半的指令**比写失败更糟。
static ATOMIC_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("corex-directive");
    let (tmp, mut file) = (0..32)
        .find_map(|_| {
            let sequence = ATOMIC_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let candidate =
                path.with_file_name(format!(".{name}.tmp-{}-{sequence}", std::process::id()));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => Some(Ok((candidate, file))),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .ok_or_else(|| anyhow::anyhow!("无法创建唯一临时文件 {}", path.display()))?
        .with_context(|| format!("无法写入 {}", path.display()))?;
    let saved = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path).context("无法替换目标文件")
    })();
    if let Err(e) = saved {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("无法保存 {}", path.display()));
    }
    Ok(())
}

/// 保证 `path`（拼接相对路径之后）解析在 `root` 之下。
fn confine_under(root: &Path, path: &Path) -> Result<PathBuf> {
    corex_core::path::confine_under(root, path).map_err(|e| anyhow::anyhow!(e.0))
}

fn resolve_dir(directives_dir: &Path, dir: Option<&str>) -> Result<PathBuf> {
    match dir {
        None => Ok(directives_dir.to_path_buf()),
        Some(d) => {
            let confined = confine_under(directives_dir, Path::new(d))?;
            if !confined.is_dir() {
                bail!("不是目录: {}", confined.display());
            }
            Ok(confined)
        }
    }
}

/// 交给宿主的路径：去掉 `\\?\` 前缀、统一成原生分隔符。
///
/// 宿主会把它显示出来、也可能直接拿去开文件，所以它得是 `cmd` / PowerShell 认的那一份
/// （`canonicalize` 之后的 `\\?\C:\...` 会被它们当成找不到路径）。
fn for_host(path: &Path) -> String {
    corex_core::path::display_path(&corex_core::path::for_external_process(path.to_path_buf()))
}

/// 列出目录下的指令，按名字排序。
///
/// `history` 顺带把每条指令「上次跑成什么样」一起算出来：一次倒扫尾巴就够全部指令，
/// 宿主画卡片不必再逐条问一遍历史。
fn directive_entries(
    dir: &Path,
    history: Option<&ExecutionHistory>,
) -> Result<Vec<DirectiveEntry>> {
    let mut entries = Vec::new();
    if !dir.exists() {
        return Ok(entries);
    }
    let ran = history
        .map(ExecutionHistory::by_directive)
        .unwrap_or_default();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(name) = directive_stem(&path) else {
            continue;
        };
        // 解析一遍就够：分类与卡片元信息都从这份模型来
        let parsed = Directive::from_yaml_file(&path).ok();
        // 账本按 YAML 里的 `name` 记（流水线只认模型，不认文件名），查账本就得用同一个键：
        // 文件主干与 `name` 不一致时按主干查永远查不到，卡片会一直显示「未运行」。
        let ran_as = parsed
            .as_ref()
            .map(|directive| directive.name.as_str())
            .unwrap_or(name);
        entries.push(DirectiveEntry {
            name: name.to_owned(),
            path: for_host(&path),
            bucket: parsed
                .as_ref()
                .and_then(|directive| directive.bucket)
                .map(|bucket| bucket.as_str().to_owned()),
            summary: parsed.as_ref().map(|directive| DirectiveSummary {
                description: directive.description.clone(),
                step_count: directive.steps.len(),
                input_count: directive.inputs.len(),
                trigger_count: directive.triggers.len(),
            }),
            last_run: ran.get(ran_as).cloned(),
        });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// 指令文件的裸名；不是 `.yaml` / `.yml` 的都不算指令。
fn directive_stem(path: &Path) -> Option<&str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("yaml") | Some("yml") => path.file_stem()?.to_str(),
        _ => None,
    }
}

fn acquire_singleton(lock_path: &Path) -> Result<File> {
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .with_context(|| format!("无法打开锁文件 {}", lock_path.display()))?;
    file.try_lock_exclusive()
        .with_context(|| "corex-daemon 已在运行（无法获取单例锁）")?;
    Ok(file)
}

fn open_history(data: &Path, config: &RuntimeConfig) -> Result<Option<ExecutionHistory>> {
    if !config.history.enabled {
        return Ok(None);
    }
    let path = if config.history.file.is_absolute() {
        config.history.file.clone()
    } else {
        data.join(&config.history.file)
    };
    Ok(Some(
        ExecutionHistory::open(path).context("无法打开执行历史文件")?,
    ))
}

/// 本次运行实际使用的 IPC 端点。
///
/// `--socket` 优先于配置里的 `socket_path`；两者都没有就是平台默认端点（见
/// [`corex_ipc::ipc_endpoint`]）。解析与平台校验都在 `corex-ipc` 里，
/// 使 CLI 与 daemon 不可能对“端点是什么”产生分歧。
fn resolve_endpoint(cli: Option<PathBuf>, daemon: &DaemonConfig, data: &Path) -> Result<PathBuf> {
    let configured = cli.or_else(|| daemon.socket_path.clone());
    Ok(corex_ipc::resolve_endpoint(data, configured.as_deref())?)
}

fn resolve_lock_path(daemon: &DaemonConfig, data: &Path) -> PathBuf {
    match &daemon.lock_path {
        Some(p) => corex_ipc::resolve_data_relative(data, p),
        None => data.join("corex.lock"),
    }
}

/// 本次运行实际使用的 token，连同它的出处。
struct Auth {
    token: String,
    /// token 来自哪个文件。只有这种情况才值得写给连接方看；`COREX_TOKEN` 与配置里的值
    /// 属于调用方，不该被复制进一个默认权限的文件。
    file: Option<PathBuf>,
}

fn resolve_auth_token(data: &Path, daemon: &DaemonConfig) -> Result<Auth> {
    if let Ok(t) = std::env::var("COREX_TOKEN")
        && !t.is_empty()
    {
        return Ok(Auth {
            token: t,
            file: None,
        });
    }
    if let Some(t) = &daemon.token
        && !t.is_empty()
    {
        return Ok(Auth {
            token: t.clone(),
            file: None,
        });
    }
    let path = data.join("token");
    let token = read_or_create_token_file(&path)?;
    Ok(Auth {
        token,
        file: Some(path),
    })
}

fn read_or_create_token_file(path: &Path) -> Result<String> {
    if path.exists() {
        let existing = std::fs::read_to_string(path)
            .with_context(|| format!("无法读取 token 文件 {}", path.display()))?;
        let trimmed = existing.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("无法写入 token 文件 {}", path.display()))?;
        file.write_all(token.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, token.as_bytes())
            .with_context(|| format!("无法写入 token 文件 {}", path.display()))?;
    }
    Ok(token)
}

fn token_matches(provided: Option<&str>, expected: &str) -> bool {
    match provided {
        Some(p) => constant_time_eq(p.as_bytes(), expected.as_bytes()),
        None => false,
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn init_tracing(logging: &LoggingConfig) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&logging.level));
    let timer =
        tracing_subscriber::fmt::time::ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string());
    if logging.json {
        let _ = tracing_subscriber::fmt()
            .json()
            .with_timer(timer)
            .with_env_filter(filter)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_timer(timer)
            .with_env_filter(filter)
            .try_init();
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "无法监听 SIGTERM");
                ctrl_c.await;
                return;
            }
        };
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用真实的内置声明，使写错的权限要求不会悄悄溜过。
    fn store() -> ActionRegistry {
        let mut registry = ActionRegistry::new();
        registry.register_builtins();
        registry
    }

    #[test]
    fn strict_invoke_denies_file_write() {
        let cfg = RuntimeConfig {
            strict_permissions: true,
            ..Default::default()
        };
        assert!(check_invoke_allowed(&cfg, &store(), "file.write").is_err());
        assert!(check_invoke_allowed(&cfg, &store(), "shell.run").is_err());
    }

    #[test]
    fn non_strict_invoke_allows_file_write() {
        let cfg = RuntimeConfig::default();
        assert!(check_invoke_allowed(&cfg, &store(), "file.write").is_ok());
    }

    #[test]
    fn strict_invoke_allows_none_kind() {
        let cfg = RuntimeConfig {
            strict_permissions: true,
            ..Default::default()
        };
        assert!(check_invoke_allowed(&cfg, &store(), "template.render").is_ok());
        assert!(check_invoke_allowed(&cfg, &store(), "generate.uuid").is_ok());
    }

    #[test]
    fn invoke_denied_when_action_disabled() {
        let cfg = RuntimeConfig {
            plugins: corex_core::PluginConfig {
                disabled_actions: vec!["shell.run".into()],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(check_invoke_allowed(&cfg, &store(), "shell.run").is_err());
        assert!(check_invoke_allowed(&cfg, &store(), "template.render").is_ok());
    }

    #[test]
    fn interactive_resources_serialize_ui_and_capture_only() {
        let registry = store();
        assert!(needs_interactive(registry.permissions_of("ui.window.list")));
        assert!(needs_interactive(
            registry.permissions_of("capture.monitors")
        ));
        assert!(!needs_interactive(
            registry.permissions_of("compression.compress")
        ));

        let directive = Directive::from_yaml_str(
            r#"
name: resources
steps:
  - id: capture
    action: capture.monitors
"#,
        )
        .unwrap();
        assert!(directive_needs_interactive(&registry, &directive));

        let directive = Directive::from_yaml_str(
            r#"
name: parallel-safe
steps:
  - id: compress
    action: compression.compress
"#,
        )
        .unwrap();
        assert!(!directive_needs_interactive(&registry, &directive));
    }

    #[test]
    fn write_atomic_does_not_reuse_a_shared_temp_name() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("example.yaml");
        let conventional_tmp = target.with_extension("tmp");
        std::fs::write(&conventional_tmp, "keep me").unwrap();

        write_atomic(&target, "new contents").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new contents");
        assert_eq!(
            std::fs::read_to_string(&conventional_tmp).unwrap(),
            "keep me"
        );
    }

    /// 名字必须是**恰好一个普通组件**：空、`.`、`..`、分隔符、绝对路径、含 `..` 的都不行。
    ///
    /// `\` 要单列：它在 Unix 上不是分隔符，组件判定会放它过去，但同一条名字会跨平台被
    /// 写进 YAML / IPC 请求，不该只在 Windows 上被挡。
    #[test]
    fn directive_name_must_be_a_bare_component() {
        for bad in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "../a",
            "a/../b",
            "pack/inner",
            "..hidden",
        ] {
            assert!(check_directive_name(bad).is_err(), "{bad:?} 该被拒");
        }
        for good in ["build", "打包"] {
            assert!(check_directive_name(good).is_ok(), "{good:?} 该通过");
        }
    }

    /// Windows 的盘符相对名（`D:evil`、`D:`）会连「落点」一起算错：`join` 把 base 丢掉。
    ///
    /// 所以不只名字那道门，算落点的那道也得分掉——直接用 `join` 的地方将来多起来时，
    /// 漏掉一道就等于漏掉整条路径沙箱。
    #[cfg(windows)]
    #[test]
    fn directive_name_rejects_windows_drive_relative() {
        let dir = tempfile::tempdir().unwrap();
        for bad in [r"D:evil", "D:", r"C:\windows", r"\\server\share\x"] {
            assert!(check_directive_name(bad).is_err(), "{bad} 该被拒");
            assert!(save_target(dir.path(), bad).is_err(), "{bad} 不该算出落点");
        }
        assert_eq!(
            save_target(dir.path(), "build").unwrap(),
            dir.path().join("build.yaml")
        );
    }

    /// 失败按**类型**分桶，不看消息文本。
    ///
    /// 看文本那种写法把「文件权限不足」这类运行期失败也报成 403；而只看 `kind()` 又会把
    /// 动作的 `not_found`（文件 / 窗口没了）与引擎的 `not_found`（指令不存在）混成一个。
    #[test]
    fn failures_map_to_ipc_codes_by_kind() {
        let code = |err: anyhow::Error| classify(&err).code;

        // 门禁拒绝：403。流水线会把它包进步骤失败，包了一层也还是 403。
        assert_eq!(
            code(ActionError::PermissionDenied("strict".into()).into()),
            403
        );
        assert_eq!(code(ActionError::Disabled("shell.run".into()).into()), 403);
        assert_eq!(
            code(
                EngineError::StepFailed {
                    step: "copy".into(),
                    source: ActionError::PermissionDenied("strict".into()),
                }
                .into()
            ),
            403
        );

        // 指令不存在：404（这是引擎的 `not_found`）。
        assert_eq!(
            code(EngineError::DirectiveNotFound("nope".into()).into()),
            404
        );

        // 运行期找不到文件：500，不是 404——那是动作跑了但没成功。
        assert_eq!(
            code(
                EngineError::StepFailed {
                    step: "read".into(),
                    source: ActionError::NotFound("build.log".into()),
                }
                .into()
            ),
            500
        );

        // 消息里出现「权限」不再等于门禁拒绝。
        assert_eq!(
            code(ActionError::ExecutionFailed("权限不足: 无法写入 build.log".into()).into()),
            500
        );
        // 参数写错与指令解析失败是调用方的问题：400。
        assert_eq!(
            code(ActionError::InvalidParams("缺少 from".into()).into()),
            400
        );
        assert_eq!(code(EngineError::ParseError("坏 YAML".into()).into()), 400);
    }
}
