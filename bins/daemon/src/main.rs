//! Corex 守护进程 —— 读配置、注册内置动作、服务 IPC。

use anyhow::{Context, Result};
use clap::Parser;
use corex_core::{
    ActionError, ActionStore, DaemonConfig, EngineError, ExecutionContext, LoggingConfig, Mark,
    Observer, PermissionKind, PermissionSet, RuntimeConfig, Spot, Stream, Value,
    check_runtime_allowed,
};
use corex_engine::{
    AuditEntry, Directive, DirectiveHistory, DirectiveSource, ExecutionAudit, HistoryEntry,
    HistorySink, JobKind as EngineJobKind, JobView, Pipeline, SupervisorIo, admission,
    ensure_trigger, jobs, required_permissions, send_job, start_detached, stop_job,
    supervise_cron_job, supervise_watch_job, validate_allowed, validate_registered,
};
use corex_ipc::protocol::{JobKind, Request, Response, RpcError};
use corex_ipc::{FrameSink, Outlet, ProgressEvent, config_paths, data_dir, serve_ipc_ready};
use corex_registry::ActionRegistry;
use corex_store::{
    BootstrapOptions, DirectiveRecord, DirectiveStore, ImportEntry, ImportOptions, ImportReport,
    ImportStatus, StoreDirectiveSource, StoreError, database_path, history_sink,
};
use fs2::FileExt;
use rand::RngExt;
use serde::Serialize;
use std::fs::File;
use std::path::{Path, PathBuf};
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

    /// 启动时把该目录（或文件）的 YAML 导入指令库，等价于 `corex directive import`
    #[arg(long = "import", alias = "directives")]
    import: Option<PathBuf>,

    /// 配置文件（toml）
    #[arg(long)]
    config: Option<PathBuf>,

    /// 作为已登记作业的 supervisor 运行（由 `start_job` 拉起，不听 IPC）
    #[arg(long, requires_all = ["kind", "job_id"])]
    supervised: bool,
    /// supervisor 作业族
    #[arg(long, value_enum)]
    kind: Option<SupervisedKind>,
    /// supervisor 作业 id（指令名）
    #[arg(long)]
    job_id: Option<String>,
    /// watch：先立刻触发一次
    #[arg(long)]
    immediate: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum SupervisedKind {
    Watch,
    Cron,
}

impl From<SupervisedKind> for EngineJobKind {
    fn from(kind: SupervisedKind) -> Self {
        match kind {
            SupervisedKind::Watch => EngineJobKind::Watch,
            SupervisedKind::Cron => EngineJobKind::Cron,
        }
    }
}

struct DaemonState {
    registry: Arc<ActionRegistry>,
    config: RuntimeConfig,
    /// 指令库所在的数据目录：supervisor 的 meta 写在这里。
    data: PathBuf,
    /// 启动时的 `--config`，传给 supervisor 子进程以免读到另一份配置。
    config_path: Option<PathBuf>,
    /// 指令的唯一真相源。指令、执行日志、上次执行时间都在这一处，谁读谁写都只经过它。
    store: Arc<DirectiveStore>,
    /// 交给触发器等「按名字取指令」的口，与 [`Self::store`] 是同一个库。
    source: Arc<dyn DirectiveSource>,
    /// `[history]` 关掉时没有：那时执行日志不记，但指令照跑。
    history: Option<Arc<dyn HistorySink>>,
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

    if args.supervised {
        return run_supervised(&args, &data, config).await;
    }

    let endpoint = resolve_endpoint(args.socket, &config.daemon, &data)?;
    let lock_path = resolve_lock_path(&config.daemon, &data);

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

    let registry = Arc::new(registry);
    let validate = admission(Arc::clone(&registry));
    let (store, report) = DirectiveStore::open_in_data_dir(
        &data,
        BootstrapOptions::from_config(&data, &config),
        &validate,
    )
    .context("无法打开指令库")?;
    let store = Arc::new(store);
    if !report.is_quiet() {
        info!(
            seeded = report.seeded.len(),
            imported = report.history_imported.unwrap_or(0),
            "指令库初始化完成"
        );
        for name in &report.seeded {
            info!(name, "已写入起步指令");
        }
        for entry in report.legacy.iter().flat_map(|legacy| &legacy.entries) {
            if let ImportStatus::Failed(reason) = &entry.status {
                warn!(path = %entry.path.display(), reason, "旧指令导入失败");
            }
        }
    }
    info!(directives = store.count().unwrap_or(0), "指令库已就绪");

    // 命令行显式指的目录：当成一次导入，而不是「换一个指令根」——指令的根只有一个，就是库。
    if let Some(target) = &args.import {
        match store.import_path(target, &ImportOptions::default(), &validate) {
            Ok(report) => info!(
                path = %target.display(),
                created = report.created(),
                updated = report.updated(),
                skipped = report.skipped(),
                failed = report.failed(),
                "已导入指令"
            ),
            Err(e) => warn!(path = %target.display(), error = %e, "导入指令失败"),
        }
    }

    let history = history_sink(Arc::clone(&store), &config);
    let audit = ExecutionAudit::under_data_dir(&data).ok();

    // `max_jobs = 0` 表示不限：拿信号量的最大许可数当“无限”。
    let jobs = Arc::new(Semaphore::new(match config.daemon.max_jobs {
        0 => Semaphore::MAX_PERMITS,
        n => n,
    }));
    // UI 与 capture 是一类共享资源；其它动作仍只受 max_jobs 限制，可以并行。
    let interactive = Arc::new(Semaphore::new(1));

    let state = Arc::new(DaemonState {
        registry,
        config,
        data: data.clone(),
        config_path: args.config.clone(),
        source: Arc::new(StoreDirectiveSource::new(Arc::clone(&store))),
        store,
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
        Request::Directives { id, .. } => match directives(state) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::ListRuns {
            id, name, limit, ..
        } => match list_runs(state, name.as_deref(), limit) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::ReadDirective { id, name, .. } => match read_directive(state, &name) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::SaveDirective {
            id,
            name,
            definition,
            original_name,
            ..
        } => match save_directive(state, &name, original_name.as_deref(), definition) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::DeleteDirective { id, name, .. } => match delete_directive(state, &name) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::ImportDirectives {
            id,
            path,
            folder,
            is_overwrite,
            is_dry_run,
            ..
        } => match import_directives(state, &path, folder.as_deref(), is_overwrite, is_dry_run) {
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
        Request::Jobs { id, kind, .. } => match jobs_reply(state, kind) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::StartJob {
            id,
            kind,
            name,
            immediate,
            ..
        } => match spawn_job(state, kind, &name, immediate) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::StopJob {
            id,
            kind,
            name,
            force,
            ..
        } => match halt_job(state, kind, &name, force).await {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::RestartJob {
            id, kind, name, ..
        } => match restart_job(state, kind, &name).await {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
        },
        Request::SendJob {
            id,
            kind,
            name,
            command,
            ..
        } => match control_job(state, kind, &name, &command) {
            Ok(data) => Response::ok(id, data),
            Err(e) => Response::error(id, e),
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
        "conflict" => RpcError::conflict(message),
        _ => RpcError::internal(message),
    }
}

fn as_engine(kind: JobKind) -> EngineJobKind {
    match kind {
        JobKind::Watch => EngineJobKind::Watch,
        JobKind::Cron => EngineJobKind::Cron,
    }
}

#[derive(Serialize)]
struct JobsReply {
    jobs: Vec<JobView>,
}

fn jobs_reply(state: &DaemonState, kind: Option<JobKind>) -> Result<Value, RpcError> {
    as_data(&JobsReply {
        jobs: jobs(&state.data, kind.map(as_engine)),
    })
}

/// `--supervised` 子进程参数（与 CLI `watch/cron run --supervised` 同形）。
fn supervised_args(
    state: &DaemonState,
    kind: EngineJobKind,
    id: &str,
    immediate: bool,
) -> Vec<String> {
    let mut args = vec![
        "--supervised".into(),
        "--kind".into(),
        kind.as_str().into(),
        "--job-id".into(),
        id.into(),
    ];
    if let Some(config) = &state.config_path {
        args.push("--config".into());
        args.push(config.display().to_string());
    }
    if immediate && kind == EngineJobKind::Watch {
        args.push("--immediate".into());
    }
    args
}

fn spawn_job(
    state: &DaemonState,
    kind: JobKind,
    name: &str,
    immediate: bool,
) -> Result<Value, RpcError> {
    let kind = as_engine(kind);
    let directive = state.source.load(name).map_err(|e| classify(&e.into()))?;
    ensure_trigger(kind, &directive).map_err(|e| classify(&e.into()))?;
    let exe = std::env::current_exe().map_err(|e| RpcError::internal(e.to_string()))?;
    let args = supervised_args(state, kind, &directive.name, immediate);
    let meta = start_detached(
        &state.data,
        kind,
        &directive.name,
        database_path(&state.data),
        &exe,
        &args,
    )
    .map_err(|e| classify(&e.into()))?;
    as_data(&JobView::from_meta(meta))
}

async fn halt_job(
    state: &DaemonState,
    kind: JobKind,
    name: &str,
    force: bool,
) -> Result<Value, RpcError> {
    let view = stop_job(&state.data, as_engine(kind), name, force)
        .await
        .map_err(|e| classify(&e.into()))?;
    as_data(&view)
}

async fn restart_job(state: &DaemonState, kind: JobKind, name: &str) -> Result<Value, RpcError> {
    if let Err(err) = stop_job(&state.data, as_engine(kind), name, false).await {
        warn!(error = %err, "停止现有作业失败，仍将尝试启动");
    }
    tokio::time::sleep(Duration::from_millis(800)).await;
    spawn_job(state, kind, name, false)
}

fn control_job(
    state: &DaemonState,
    kind: JobKind,
    name: &str,
    command: &str,
) -> Result<Value, RpcError> {
    let view = send_job(&state.data, as_engine(kind), name, command)
        .map_err(|e| classify(&e.into()))?;
    as_data(&view)
}

/// `start_job` 拉起的子进程：只跑 supervisor，不占 IPC 锁。
async fn run_supervised(args: &Args, data: &Path, config: RuntimeConfig) -> Result<()> {
    let kind = EngineJobKind::from(args.kind.expect("--supervised 需要 --kind"));
    let job_id = args.job_id.as_deref().expect("--supervised 需要 --job-id");
    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    registry.remove_disabled(&config.plugins);
    {
        let plugin_dir = if config.plugins.plugin_dir.is_absolute() {
            config.plugins.plugin_dir.clone()
        } else {
            data.join(&config.plugins.plugin_dir)
        };
        if let Err(e) = corex_registry::discovery::discover(&plugin_dir, &mut registry) {
            warn!(error = %e, "插件发现失败");
        }
    }
    let registry = Arc::new(registry);
    let validate = admission(Arc::clone(&registry));
    let (store, _) = DirectiveStore::open_in_data_dir(
        data,
        BootstrapOptions::from_config(data, &config),
        &validate,
    )
    .context("无法打开指令库")?;
    let store = Arc::new(store);
    let history = history_sink(Arc::clone(&store), &config);
    let source: Arc<dyn DirectiveSource> = Arc::new(StoreDirectiveSource::new(Arc::clone(&store)));
    let io = SupervisorIo {
        source: Some(source),
        history,
    };
    let meta = corex_engine::JobMeta::read(data, kind, job_id).with_context(|| {
        format!("读取 {} job `{job_id}` 的 meta", kind.as_str())
    })?;
    info!(job = job_id, kind = kind.as_str(), "supervisor 已接管");
    match kind {
        EngineJobKind::Watch => {
            supervise_watch_job(&meta, registry, config, data, args.immediate, &io).await?;
        }
        EngineJobKind::Cron => {
            supervise_cron_job(&meta, registry, config, data, &io).await?;
        }
    }
    Ok(())
}

/// 指令库里的一条指令。
///
/// `name` 是后续 `read_directive` / `save_directive` 要的键；`folder` / `source` /
/// `updated_at_ms` 是库里的元信息，宿主拿它分组、显示「从哪来、什么时候改的」。分类与描述
/// 得先解析模型才知道，所以**解析不了的条目照样列出来**（编辑器要能打开它去修），只是
/// `summary` 为空。
#[derive(Serialize)]
struct DirectiveEntry {
    name: String,
    /// 分组（自由文本；导入时取相对子目录）。`None` = 未分组。
    folder: Option<String>,
    /// 导入来源（当初那份 YAML 的路径）；库里新建的没有。
    source: Option<String>,
    /// 是否出现在用户指令列表。
    visible: bool,
    updated_at_ms: u64,
    /// 动作分类（`system` / `network` / …），卡片按它分桶。
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

/// 卡片要用的元信息：宿主列目录时不该为了显示描述、步骤数再把每条模型读一遍。
#[derive(Serialize)]
struct DirectiveSummary {
    description: String,
    step_count: usize,
    input_count: usize,
    trigger_count: usize,
    has_cron: bool,
    has_watch: bool,
}

/// 一条指令的规范化 YAML 与模型。
///
/// 两个都给：宿主展示与「保留自己没改的字段」要用原文，编辑要用模型。让宿主自己
/// 解析一遍 YAML 的话，编辑器里的形状迟早会和真跑的那份不一样。
#[derive(Serialize)]
struct DirectiveDocument {
    name: String,
    folder: Option<String>,
    source: Option<String>,
    visible: bool,
    created_at_ms: u64,
    updated_at_ms: u64,
    /// 库序列化出来的规范 YAML。宿主**不要**把它当输入再拼一遍——写出去只有引擎一份。
    yaml: String,
    definition: Directive,
}

impl DirectiveDocument {
    fn of(record: DirectiveRecord) -> Self {
        Self {
            name: record.name,
            folder: record.folder,
            source: record.source,
            visible: record.visible,
            created_at_ms: record.created_at_ms,
            updated_at_ms: record.updated_at_ms,
            yaml: record.yaml,
            definition: record.definition,
        }
    }
}

/// 一次导入里单个条目的结果。
#[derive(Serialize)]
struct ImportEntryReply {
    name: String,
    path: String,
    /// `created` / `updated` / `skipped` / `failed`。
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ImportEntryReply {
    fn of(entry: ImportEntry) -> Self {
        let (status, error) = match entry.status {
            ImportStatus::Created => ("created", None),
            ImportStatus::Updated => ("updated", None),
            ImportStatus::Skipped => ("skipped", None),
            ImportStatus::Failed(reason) => ("failed", Some(reason)),
        };
        Self {
            name: entry.name,
            path: entry.path.display().to_string(),
            status,
            error,
        }
    }
}

/// 一次导入的回话：逐条报告，外加四个计数。
///
/// 逐条而不是「成功几条、失败几条」：导入是用户拿自己的文件来换库里的内容，「哪个文件为什么
/// 没进来」必须能一眼看到。
#[derive(Serialize)]
struct ImportReply {
    entries: Vec<ImportEntryReply>,
    created: usize,
    updated: usize,
    skipped: usize,
    failed: usize,
}

impl ImportReply {
    fn of(report: ImportReport) -> Self {
        let (created, updated, skipped, failed) = (
            report.created(),
            report.updated(),
            report.skipped(),
            report.failed(),
        );
        Self {
            entries: report
                .entries
                .into_iter()
                .map(ImportEntryReply::of)
                .collect(),
            created,
            updated,
            skipped,
            failed,
        }
    }
}

fn directives(state: &DaemonState) -> Result<Value, RpcError> {
    // 账本一次倒扫就够全部指令：宿主画卡片不必再逐条问一遍历史。
    let ran = state
        .history
        .as_ref()
        .map(|history| history.by_directive())
        .unwrap_or_default();
    let directives: Vec<DirectiveEntry> = state
        .store
        .metas()
        .map_err(from_store)?
        .into_iter()
        .map(|meta| DirectiveEntry {
            bucket: meta
                .summary
                .as_ref()
                .and_then(|summary| summary.bucket.clone()),
            summary: meta.summary.map(|summary| DirectiveSummary {
                description: summary.description,
                step_count: summary.step_count,
                input_count: summary.input_count,
                trigger_count: summary.trigger_count,
                has_cron: summary.has_cron,
                has_watch: summary.has_watch,
            }),
            last_run: ran.get(&meta.name).cloned(),
            name: meta.name,
            folder: meta.folder,
            source: meta.source,
            visible: meta.visible,
            updated_at_ms: meta.updated_at_ms,
        })
        .collect();
    as_data(&directives)
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

fn read_directive(state: &DaemonState, name: &str) -> Result<Value, RpcError> {
    let record = state.store.fetch(name).map_err(from_store)?;
    as_data(&DirectiveDocument::of(record))
}

/// 保存一条指令，并把**规范化之后**的那份还给宿主。
///
/// 先过 `run` 走的那**两道门**再落库：动作没注册是 400、权限声明不够是 403，两者
/// 写进去都跑不起来，不如当场拒掉——免得宿主存下一份注定失败的指令。
///
/// `original_name` 与 `name` 不同就是**改名**：库在一个事务里删旧键、写新键并沿用分组与
/// 来源。v12 那套「拿新名字另存一份文件、旧文件留在目录里」的操作就此消失。
fn save_directive(
    state: &DaemonState,
    name: &str,
    original_name: Option<&str>,
    definition: Value,
) -> Result<Value, RpcError> {
    let directive: Directive = serde_json::from_value(definition.to_json())
        .map_err(|e| RpcError::invalid(format!("指令定义不合法: {e}")))?;
    validate_registered(&*state.registry, &directive).map_err(|e| from_engine(&e))?;
    validate_allowed(&*state.registry, &directive).map_err(|e| from_action(&e))?;
    let record = state
        .store
        .save_with_rename(original_name, name, &directive)
        .map_err(from_store)?;
    as_data(&DirectiveDocument::of(record))
}

fn delete_directive(state: &DaemonState, name: &str) -> Result<Value, RpcError> {
    state.store.delete(name).map_err(from_store)?;
    as_data(&serde_json::json!({ "name": name }))
}

/// 从磁盘导入指令（YAML 文件或目录）。
///
/// 这是 v13 里 YAML 的入口：`path` 是文件就导入一条，是目录就递归导入，相对子目录成为分组。
fn import_directives(
    state: &DaemonState,
    path: &str,
    folder: Option<&str>,
    is_overwrite: bool,
    is_dry_run: bool,
) -> Result<Value, RpcError> {
    let opts = ImportOptions {
        is_overwrite,
        is_dry_run,
        folder: folder.map(str::to_owned),
    };
    let report = state
        .store
        .import_path(
            Path::new(path),
            &opts,
            &admission(Arc::clone(&state.registry)),
        )
        .map_err(from_store)?;
    as_data(&ImportReply::of(report))
}

/// 序列化成一条响应的 `data`。
fn as_data<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value)
        .map(Value::from_json)
        .map_err(|e| RpcError::internal(e.to_string()))
}

/// 库的错误按**类型**分桶，不看消息文本：名字撞车给 409（调用方要换个名字或先删旧的），
/// 库里没有给 404，名字/定义不合法给 400，其余是 daemon 自己的问题（500）。
fn from_store(err: StoreError) -> RpcError {
    let message = err.to_string();
    match err {
        StoreError::NotFound(_) => RpcError::not_found(message),
        StoreError::NameTaken(_) => RpcError::conflict(message),
        StoreError::InvalidName(_) | StoreError::Invalid(_) => RpcError::invalid(message),
        StoreError::Sql(_) | StoreError::Io(_) => RpcError::internal(message),
    }
}

/// 这次执行要跑的那条指令。
///
/// `path` 是调用方显式给的 **ad-hoc 文件**（`corex run --file`、MCP 的临时文件）：给了文件就
/// 认它，不再往库里找；没给就从库里按名字取。库里没有的名字由 [`from_engine`] 报 404。
fn load_directive(state: &DaemonState, name: &str, path: Option<&str>) -> Result<Directive> {
    match path {
        Some(p) => Ok(Directive::from_yaml_file(Path::new(p))?),
        None => Ok(state.source.load(name)?),
    }
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

    /// 库的错误按类型分桶：404 / 409 / 400 / 500。
    ///
    /// 409 与 400 必须分开：400 要改自己发的内容，409 得换个名字或先删旧的——宿主能做的事
    /// 完全不同，合成一个就只能靠猜消息文本。
    #[test]
    fn store_failures_map_to_ipc_codes_by_kind() {
        let code = |err: StoreError| from_store(err).code;

        assert_eq!(code(StoreError::NotFound("build".into())), 404);
        assert_eq!(code(StoreError::NameTaken("build".into())), 409);
        assert_eq!(code(StoreError::InvalidName("a/b".into())), 400);
        assert_eq!(code(StoreError::Invalid("坏 YAML".into())), 400);
        assert_eq!(code(StoreError::Io(std::io::Error::other("磁盘满了"))), 500);
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
