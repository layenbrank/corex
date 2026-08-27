//! Corex daemon — loads config, registers builtins, serves IPC.

use anyhow::{bail, Context, Result};
use clap::Parser;
use corex_core::{DaemonConfig, ExecutionContext, LoggingConfig, RuntimeConfig, Value, RUNTIME_CONFIG};
use corex_engine::{
    permission_kind_for, AuditEntry, ExecutionAudit, ExecutionHistory, PermissionKind, Pipeline,
    Directive,
};
use corex_ipc::protocol::{Request, Response, RpcError};
use corex_ipc::{platform_data_dir, platform_endpoint, serve_platform};
use corex_registry::ActionRegistry;
use fs2::FileExt;
use rand::RngExt;
use std::collections::BTreeMap;
#[cfg(unix)]
use std::io::Write;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
#[cfg(unix)]
use tracing::error;

#[derive(Parser, Debug)]
#[command(name = "corex-daemon", version, about = "Corex background daemon")]
struct Args {
    /// Override IPC endpoint (Unix socket path, or Windows named pipe e.g. \\.\pipe\corex)
    #[arg(long, alias = "pipe")]
    socket: Option<PathBuf>,

    /// Override directives directory
    #[arg(long)]
    directives: Option<PathBuf>,

    /// Config file (toml)
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

    let data = platform_data_dir()?;
    let config = load_runtime_config(args.config.as_deref())?;
    init_tracing(&config.logging);

    let endpoint = resolve_endpoint(args.socket, &config.daemon, &data);
    let lock_path = resolve_lock_path(&config.daemon, &data);
    let directives_dir = args
        .directives
        .unwrap_or_else(|| data.join("directives"));
    std::fs::create_dir_all(&directives_dir)?;

    let auth_token = resolve_auth_token(&data, &config.daemon)?;

    let _lock = acquire_singleton(&lock_path)?;

    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    registry.apply_runtime_config(&config);
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

    // Signal handling
    let flag = Arc::clone(&state);
    tokio::spawn(async move {
        shutdown_signal().await;
        info!("收到停止信号");
        flag.shutdown.store(true, Ordering::SeqCst);
        // Best-effort: remove socket so serve loop errors out / clients fail fast.
    });

    info!(endpoint = %endpoint.display(), "corex-daemon 启动");

    let state_serve = Arc::clone(&state);
    let result = serve_platform(&endpoint, move |req| {
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
            let list: Vec<Value> = state
                .registry
                .list()
                .into_iter()
                .map(|m| {
                    let mut map = BTreeMap::new();
                    map.insert("id".into(), Value::Str(m.id));
                    map.insert("name".into(), Value::Str(m.name));
                    map.insert("description".into(), Value::Str(m.description));
                    Value::Map(map)
                })
                .collect();
            Response::ok(id, Value::List(list))
        }
        Request::ListDirectives { id, dir, .. } => {
            match resolve_list_dir(&state.directives_dir, dir.as_deref()) {
                Ok(base) => match list_directives(&base) {
                    Ok(names) => {
                        let list = names.into_iter().map(Value::Str).collect();
                        Response::ok(id, Value::List(list))
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
            id,
            action,
            params,
            ..
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
    check_invoke_allowed(&state.config, action_id)?;
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
    let perm_denied = matches!(
        &outcome,
        Err(corex_core::ActionError::PermissionDenied(_))
    );
    if let Some(audit) = &state.audit {
        let entry = AuditEntry::new(
            "invoke",
            "invoke",
            action_id,
            duration_ms,
            outcome.as_ref().map(|_| ()).map_err(|e| e.to_string()),
            perm_denied,
        );
        audit.record_best_effort(&entry);
    }
    Ok(outcome?)
}

/// Strict mode: Invoke has no Directive permissions context, so any action that
/// requires a permission kind is denied. Disabled actions are already removed
/// from the registry via `apply_runtime_config`.
fn check_invoke_allowed(config: &RuntimeConfig, action_id: &str) -> Result<()> {
    if config.strict_permissions && permission_kind_for(action_id) != PermissionKind::None {
        bail!("strict_permissions: Invoke 不允许执行需权限的动作 {action_id}");
    }
    Ok(())
}

/// Resolve a Directive by name: only `{name}.yaml` / `{name}.yml` under `dir`.
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

/// Ensure `path` resolves under `root` (after joining relative paths).
fn confine_under(root: &Path, path: &Path) -> Result<PathBuf> {
    corex_core::path::confine_under(root, path).map_err(|e| anyhow::anyhow!(e.0))
}

fn resolve_list_dir(directives_dir: &Path, dir: Option<&str>) -> Result<PathBuf> {
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

fn list_directives(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    if !dir.exists() {
        return Ok(names);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) {
            if let Some(stem) = path.file_stem() {
                names.push(stem.to_string_lossy().to_string());
            }
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

/// Resolve a path from config: absolute stays absolute; relative joins `data`.
/// On Windows, `\\.\pipe\...` (and `//./pipe/...`) are used as-is.
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
    platform_endpoint(data)
}

fn resolve_lock_path(daemon: &DaemonConfig, data: &Path) -> PathBuf {
    match &daemon.lock_path {
        Some(p) => resolve_data_relative(data, p),
        None => data.join("corex.lock"),
    }
}

fn resolve_auth_token(data: &Path, daemon: &DaemonConfig) -> Result<String> {
    if let Ok(t) = std::env::var("COREX_TOKEN") {
        if !t.is_empty() {
            return Ok(t);
        }
    }
    if let Some(t) = &daemon.token {
        if !t.is_empty() {
            return Ok(t.clone());
        }
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

fn load_runtime_config(path: Option<&Path>) -> Result<RuntimeConfig> {
    let candidates: Vec<PathBuf> = path.map(|p| vec![p.to_path_buf()]).unwrap_or_else(|| {
        vec![
            PathBuf::from(RUNTIME_CONFIG),
            platform_data_dir()
                .map(|d| d.join("config.toml"))
                .unwrap_or_default(),
        ]
    });

    for p in candidates {
        if p.exists() {
            let text = std::fs::read_to_string(&p)?;
            match toml::from_str::<ConfigFile>(&text) {
                Ok(cf) => return Ok(cf.into_runtime()),
                Err(e) => warn!(path = %p.display(), error = %e, "配置解析失败，尝试下一个"),
            }
        }
    }
    Ok(RuntimeConfig::default())
}

#[derive(Debug, serde::Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    plugins: Option<corex_core::PluginConfig>,
    #[serde(default)]
    history: Option<corex_core::HistoryConfig>,
    #[serde(default)]
    daemon: Option<DaemonConfig>,
    #[serde(default)]
    logging: Option<LoggingConfig>,
    #[serde(default)]
    runtime: Option<RuntimeSection>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct RuntimeSection {
    #[serde(default)]
    max_parallel: Option<usize>,
    #[serde(default)]
    step_timeout_secs: Option<u64>,
    #[serde(default)]
    strict_permissions: Option<bool>,
    #[serde(default)]
    filesystem_roots: Option<Vec<std::path::PathBuf>>,
    #[serde(default)]
    ui_profile: Option<String>,
    #[serde(default)]
    ui_max_selector_chain: Option<usize>,
    #[serde(default)]
    ui_max_settle_ms: Option<u64>,
}

impl ConfigFile {
    fn into_runtime(self) -> RuntimeConfig {
        let mut cfg = RuntimeConfig::default();
        if let Some(p) = self.plugins {
            cfg.plugins = p;
        }
        if let Some(h) = self.history {
            cfg.history = h;
        }
        if let Some(d) = self.daemon {
            cfg.daemon = d;
        }
        if let Some(l) = self.logging {
            cfg.logging = l;
        }
        if let Some(r) = self.runtime {
            if let Some(m) = r.max_parallel {
                cfg.max_parallel = m;
            }
            if let Some(t) = r.step_timeout_secs {
                cfg.step_timeout_secs = t;
            }
            if let Some(s) = r.strict_permissions {
                cfg.strict_permissions = s;
            }
            if let Some(roots) = r.filesystem_roots {
                cfg.filesystem_roots = roots;
            }
            let overrides = corex_core::UiProfileOverrides {
                max_selector_chain: r.ui_max_selector_chain,
                max_settle_ms: r.ui_max_settle_ms,
            };
            if let Some(profile) = r.ui_profile {
                cfg.apply_ui_profile(&profile, overrides);
            } else {
                if let Some(n) = r.ui_max_selector_chain {
                    cfg.ui_max_selector_chain = n;
                }
                if let Some(ms) = r.ui_max_settle_ms {
                    cfg.ui_max_settle_ms = ms;
                }
            }
        }
        cfg
    }
}

fn init_tracing(logging: &LoggingConfig) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&logging.level));
    if logging.json {
        let _ = tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
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
        use tokio::signal::unix::{signal, SignalKind};
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

    #[test]
    fn strict_invoke_denies_file_write() {
        let mut cfg = RuntimeConfig::default();
        cfg.strict_permissions = true;
        assert!(check_invoke_allowed(&cfg, "file.write").is_err());
        assert!(check_invoke_allowed(&cfg, "shell.run").is_err());
    }

    #[test]
    fn non_strict_invoke_allows_file_write() {
        let cfg = RuntimeConfig::default();
        assert!(check_invoke_allowed(&cfg, "file.write").is_ok());
    }

    #[test]
    fn strict_invoke_allows_none_kind() {
        let mut cfg = RuntimeConfig::default();
        cfg.strict_permissions = true;
        assert!(check_invoke_allowed(&cfg, "template.render").is_ok());
        assert!(check_invoke_allowed(&cfg, "generate.uuid").is_ok());
    }
}
