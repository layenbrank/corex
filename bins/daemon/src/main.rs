//! Corex daemon — loads config, registers builtins, serves IPC.

use anyhow::{bail, Context, Result};
use clap::Parser;
use corex_core::{ExecutionContext, RuntimeConfig, Value};
use corex_engine::{ExecutionHistory, Pipeline, Shortcut};
use corex_ipc::protocol::{Request, Response, RpcError};
use corex_ipc::{default_endpoint, serve_platform};
use corex_registry::ActionRegistry;
use fs2::FileExt;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "corex-daemon", version, about = "Corex background daemon")]
struct Args {
    /// Override IPC endpoint (Unix socket path, or Windows named pipe e.g. \\.\pipe\corex)
    #[arg(long, alias = "pipe")]
    socket: Option<PathBuf>,

    /// Override shortcuts directory
    #[arg(long)]
    shortcuts: Option<PathBuf>,

    /// Config file (toml)
    #[arg(long)]
    config: Option<PathBuf>,
}

struct DaemonState {
    registry: Arc<ActionRegistry>,
    config: RuntimeConfig,
    shortcuts_dir: PathBuf,
    history: Option<ExecutionHistory>,
    shutdown: AtomicBool,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let args = Args::parse();

    let data = data_dir()?;
    let config = load_runtime_config(args.config.as_deref())?;
    let endpoint = args
        .socket
        .unwrap_or_else(|| default_endpoint(&data));
    let lock_path = data.join("corex.lock");
    let shortcuts_dir = args
        .shortcuts
        .unwrap_or_else(|| data.join("shortcuts"));
    std::fs::create_dir_all(&shortcuts_dir)?;

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

    let state = Arc::new(DaemonState {
        registry: Arc::new(registry),
        config,
        shortcuts_dir,
        history,
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
    if state.shutdown.load(Ordering::SeqCst) {
        return Response::Bye { id };
    }

    match req {
        Request::Ping { id } => Response::Pong { id },
        Request::Shutdown { id } => {
            state.shutdown.store(true, Ordering::SeqCst);
            Response::Bye { id }
        }
        Request::ListActions { id } => {
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
        Request::ListShortcuts { id, dir } => {
            let base = dir
                .map(PathBuf::from)
                .unwrap_or_else(|| state.shortcuts_dir.clone());
            match list_shortcuts(&base) {
                Ok(names) => {
                    let list = names.into_iter().map(Value::Str).collect();
                    Response::ok(id, Value::List(list))
                }
                Err(e) => Response::error(id, RpcError::internal(e.to_string())),
            }
        }
        Request::RunShortcut {
            id,
            name,
            input,
            path,
        } => match run_shortcut(state, &name, path.as_deref(), input).await {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::error(id, RpcError::internal(e.to_string())),
        },
        Request::Invoke { id, action, params } => match invoke_action(state, &action, params).await
        {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::error(id, RpcError::internal(e.to_string())),
        },
    }
}

async fn run_shortcut(
    state: &DaemonState,
    name: &str,
    path: Option<&str>,
    input: std::collections::HashMap<String, Value>,
) -> Result<Value> {
    let file = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        resolve_shortcut(&state.shortcuts_dir, name)?
    };
    let shortcut = Shortcut::from_yaml_file(&file)?;
    let ctx = ExecutionContext::new(state.config.clone()).with_input(input);
    let mut pipeline = Pipeline::new(state.registry.clone());
    if let Some(history) = &state.history {
        pipeline = pipeline.with_history(history.clone());
    }
    Ok(pipeline.execute(&shortcut, ctx).await?)
}

async fn invoke_action(state: &DaemonState, action_id: &str, params: Value) -> Result<Value> {
    let action = state
        .registry
        .get(action_id)
        .with_context(|| format!("动作未注册: {action_id}"))?;
    let mut ctx = ExecutionContext::new(state.config.clone());
    action.validate(&params).await?;
    Ok(action.execute(params, &mut ctx).await?)
}

fn resolve_shortcut(dir: &Path, name: &str) -> Result<PathBuf> {
    let candidates = [
        dir.join(format!("{name}.yaml")),
        dir.join(format!("{name}.yml")),
        PathBuf::from(name),
    ];
    for c in candidates {
        if c.exists() {
            return Ok(c);
        }
    }
    bail!("快捷指令未找到: {name}");
}

fn list_shortcuts(dir: &Path) -> Result<Vec<String>> {
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

fn data_dir() -> Result<PathBuf> {
    let base = directories::ProjectDirs::from("dev", "corex", "corex")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".corex"));
    std::fs::create_dir_all(&base)?;
    Ok(base)
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

fn load_runtime_config(path: Option<&Path>) -> Result<RuntimeConfig> {
    let candidates: Vec<PathBuf> = path
        .map(|p| vec![p.to_path_buf()])
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("config/default.toml"),
                data_dir()
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
    runtime: Option<RuntimeSection>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct RuntimeSection {
    #[serde(default)]
    max_parallel: Option<usize>,
    #[serde(default)]
    step_timeout_secs: Option<u64>,
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
        if let Some(r) = self.runtime {
            if let Some(m) = r.max_parallel {
                cfg.max_parallel = m;
            }
            if let Some(t) = r.step_timeout_secs {
                cfg.step_timeout_secs = t;
            }
        }
        cfg
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
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
