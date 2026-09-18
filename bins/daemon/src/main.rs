//! Corex 守护进程 —— 读配置、注册内置动作、服务 IPC。

use anyhow::{Context, Result, bail};
use clap::Parser;
use corex_core::{
    ActionError, DaemonConfig, EngineError, ExecutionContext, LoggingConfig, Mark, Observer,
    RuntimeConfig, Spot, Value, check_runtime_allowed,
};
use corex_engine::{AuditEntry, Directive, ExecutionAudit, ExecutionHistory, Pipeline};
use corex_ipc::protocol::{Request, Response, RpcError};
use corex_ipc::{FrameSink, Outlet, ProgressEvent, config_paths, data_dir, serve_ipc_ready};
use corex_registry::ActionRegistry;
use fs2::FileExt;
use rand::RngExt;
use std::fs::File;
#[cfg(unix)]
use std::io::Write;
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
    let directives_dir = args.directives.unwrap_or_else(|| data.join("directives"));
    std::fs::create_dir_all(&directives_dir)?;

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

    let state = Arc::new(DaemonState {
        registry: Arc::new(registry),
        config,
        directives_dir,
        history,
        audit,
        auth_token: auth.token,
        shutdown: AtomicBool::new(false),
        jobs,
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
}

impl Observer for StreamObserver {
    fn begin(&self, at: Spot<'_>) {
        self.outlet.frame(&ProgressEvent::StepStart {
            seq: self.seq.fetch_add(1, Ordering::Relaxed) + 1,
            step: at.id.to_string(),
            action: at.action.to_string(),
        });
    }

    fn chunk(&self, at: Spot<'_>, mark: Mark) {
        self.outlet.frame(&ProgressEvent::StepProgress {
            step: at.id.to_string(),
            action: at.action.to_string(),
            done: mark.done,
            total: mark.total,
            unit: mark.unit,
        });
    }

    fn end(&self, at: Spot<'_>, took: Duration, ok: bool) {
        self.outlet.frame(&ProgressEvent::StepEnd {
            step: at.id.to_string(),
            action: at.action.to_string(),
            took_ms: took.as_millis() as u64,
            ok,
        });
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

    // **执行类**请求排这个队；控制类（探活、状态、列目录）不排——它们是宿主判断
    // “daemon 还活着吗”的手段，被一条几分钟的指令堵住才是最糟的。
    // 信号量在本进程里从不关闭（`Arc` 与 daemon 同寿），所以错误分支只是形式上的。
    let _permit = match req {
        Request::RunDirective { .. } | Request::Invoke { .. } => match state.jobs.acquire().await {
            Ok(permit) => Some(permit),
            Err(_) => return Response::error(id, RpcError::internal("执行队列已关闭")),
        },
        _ => None,
    };

    // 只有显式置了 `stream` 的请求才建上报口：其余请求连一次额外写入也不会有。
    let observer = req
        .wants_stream()
        .then(|| Arc::new(StreamObserver::new(outlet)) as Arc<dyn Observer>);

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
        Request::ListDirectives { id, dir, .. } => {
            match resolve_dir(&state.directives_dir, dir.as_deref()) {
                Ok(base) => match directives(&base) {
                    Ok(names) => {
                        let entries = names.into_iter().map(Value::Str).collect();
                        Response::ok(id, Value::Array(entries))
                    }
                    Err(e) => Response::error(id, RpcError::internal(e.to_string())),
                },
                Err(e) => Response::error(id, RpcError::forbidden(e.to_string())),
            }
        }
        Request::RunDirective {
            id,
            name,
            input,
            path,
            ..
        } => match run_directive(state, &name, path.as_deref(), input, observer.as_ref()).await {
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

async fn run_directive(
    state: &DaemonState,
    name: &str,
    path: Option<&str>,
    input: std::collections::HashMap<String, Value>,
    observer: Option<&Arc<dyn Observer>>,
) -> Result<Value> {
    let file = if let Some(p) = path {
        confine_under(&state.directives_dir, Path::new(p))
            .with_context(|| format!("指令路径越界: {p}"))?
    } else {
        resolve_directive(&state.directives_dir, name)?
    };
    let directive = Directive::from_yaml_file(&file)?;
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
    Ok(pipeline.execute(&directive, ctx).await?)
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
fn resolve_directive(dir: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || Path::new(name).is_absolute()
    {
        bail!("非法指令名: {name}");
    }
    let yaml = dir.join(format!("{name}.yaml"));
    let yml = dir.join(format!("{name}.yml"));
    if yaml.is_file() {
        return Ok(yaml);
    }
    if yml.is_file() {
        return Ok(yml);
    }
    bail!("指令未找到: {name}");
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

fn directives(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    if !dir.exists() {
        return Ok(names);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) && let Some(stem) = path.file_stem()
        {
            names.push(stem.to_string_lossy().to_string());
        }
    }
    names.sort();
    Ok(names)
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
