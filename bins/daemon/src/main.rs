//! Corex 守护进程 —— 读配置、注册内置动作、服务 IPC。

use anyhow::{Context, Result, bail};
use clap::Parser;
use corex_core::{
    DaemonConfig, ExecutionContext, LoggingConfig, RuntimeConfig, Value, check_runtime_allowed,
};
use corex_engine::{AuditEntry, Directive, ExecutionAudit, ExecutionHistory, Pipeline};
use corex_ipc::protocol::{Request, Response, RpcError};
use corex_ipc::{config_paths, data_dir, ipc_endpoint, serve_ipc};
use corex_registry::ActionRegistry;
use fs2::FileExt;
use rand::RngExt;
use std::collections::BTreeMap;
use std::fs::File;
#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

    let endpoint = resolve_endpoint(args.socket, &config.daemon, &data);
    let lock_path = resolve_lock_path(&config.daemon, &data);
    let directives_dir = args.directives.unwrap_or_else(|| data.join("directives"));
    std::fs::create_dir_all(&directives_dir)?;

    let auth_token = resolve_auth_token(&data, &config.daemon)?;

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

    let state = Arc::new(DaemonState {
        registry: Arc::new(registry),
        config,
        directives_dir,
        history,
        audit,
        auth_token,
        shutdown: AtomicBool::new(false),
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

    let state_serve = Arc::clone(&state);
    let result = serve_ipc(&endpoint, move |req| {
        let state = Arc::clone(&state_serve);
        async move { handle_request(&state, req).await }
    })
    .await;

    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&endpoint);
    }
    info!("corex-daemon 已退出");
    result.context("IPC 服务异常")?;
    Ok(())
}

async fn handle_request(state: &DaemonState, req: Request) -> Response {
    let id = req.id();
    if !token_matches(req.auth_token(), &state.auth_token) {
        return Response::error(id, RpcError::unauthorized("invalid or missing auth token"));
    }
    if state.shutdown.load(Ordering::SeqCst) {
        return Response::Bye { id };
    }

    match req {
        Request::Ping { id, .. } => Response::Pong { id },
        Request::Shutdown { id, .. } => {
            state.shutdown.store(true, Ordering::SeqCst);
            Response::Bye { id }
        }
        Request::ListActions { id, .. } => {
            let actions: Vec<Value> = state
                .registry
                .actions()
                .into_iter()
                .map(|m| {
                    let mut map = BTreeMap::new();
                    map.insert("id".into(), Value::Str(m.id));
                    map.insert("name".into(), Value::Str(m.name));
                    map.insert("description".into(), Value::Str(m.description));
                    Value::Map(map)
                })
                .collect();
            Response::ok(id, Value::Array(actions))
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
        } => match run_directive(state, &name, path.as_deref(), input).await {
            Ok(v) => Response::ok(id, v),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("指令未找到") {
                    Response::error(id, RpcError::not_found(msg))
                } else {
                    Response::error(id, RpcError::internal(msg))
                }
            }
        },
        Request::Invoke {
            id, action, params, ..
        } => match invoke_action(state, &action, params).await {
            Ok(v) => Response::ok(id, v),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("strict_permissions") || msg.contains("权限") {
                    Response::error(id, RpcError::forbidden(msg))
                } else {
                    Response::error(id, RpcError::internal(msg))
                }
            }
        },
    }
}

async fn run_directive(
    state: &DaemonState,
    name: &str,
    path: Option<&str>,
    input: std::collections::HashMap<String, Value>,
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
    Ok(pipeline.execute(&directive, ctx).await?)
}

async fn invoke_action(state: &DaemonState, action_id: &str, params: Value) -> Result<Value> {
    check_invoke_allowed(&state.config, &*state.registry, action_id)?;
    let action = state
        .registry
        .get(action_id)
        .with_context(|| format!("动作未注册: {action_id}"))?;
    let t0 = std::time::Instant::now();
    let mut ctx = ExecutionContext::new(state.config.clone());
    let outcome = async {
        action.validate(&params).await?;
        action.execute(params, &mut ctx).await
    }
    .await;
    let duration_ms = t0.elapsed().as_millis() as u64;
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
fn check_invoke_allowed(
    config: &RuntimeConfig,
    store: &dyn corex_core::ActionStore,
    action_id: &str,
) -> Result<()> {
    check_runtime_allowed(config, store, action_id).map_err(|e| anyhow::anyhow!("{e}"))
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

/// 解析配置里的路径：绝对路径原样；相对路径拼到 `data` 下。
/// Windows 上 `\\.\pipe\...`（以及 `//./pipe/...`）那一类属于命名管道名字，直接用。
fn resolve_data_relative(data: &Path, path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if s.starts_with(r"\\.\pipe\") || s.starts_with("//./pipe/") {
            return path.to_path_buf();
        }
    }
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        data.join(path)
    }
}

fn resolve_endpoint(cli: Option<PathBuf>, daemon: &DaemonConfig, data: &Path) -> PathBuf {
    if let Some(p) = cli {
        return p;
    }
    if let Some(p) = &daemon.socket_path {
        return resolve_data_relative(data, p);
    }
    ipc_endpoint(data)
}

fn resolve_lock_path(daemon: &DaemonConfig, data: &Path) -> PathBuf {
    match &daemon.lock_path {
        Some(p) => resolve_data_relative(data, p),
        None => data.join("corex.lock"),
    }
}

fn resolve_auth_token(data: &Path, daemon: &DaemonConfig) -> Result<String> {
    if let Ok(t) = std::env::var("COREX_TOKEN")
        && !t.is_empty()
    {
        return Ok(t);
    }
    if let Some(t) = &daemon.token
        && !t.is_empty()
    {
        return Ok(t.clone());
    }
    read_or_create_token_file(&data.join("token"))
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
}
