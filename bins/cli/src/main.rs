//! Corex CLI entrypoint.

mod cron_cmd;
mod editor;
mod repl;
mod trigger_cmd;
mod ui_cmd;
mod watch_cmd;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use corex_core::{DaemonConfig, ExecutionContext, LoggingConfig, RuntimeConfig, Value, RUNTIME_CONFIG};
use corex_engine::{validate_permissions, ExecutionAudit, ExecutionHistory, Pipeline, Directive};
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{platform_data_dir, platform_endpoint, platform_transport, Transport};
use corex_registry::ActionRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "corex", version, about = "Corex — composable directives & actions")]
struct Cli {
    /// Directive / config search directory
    #[arg(long, global = true)]
    dir: Option<PathBuf>,

    /// Increase logging verbosity
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a Directive by name or file path
    Run {
        /// Directive name (without .yaml) or path to YAML
        target: String,
        /// Input as KEY=VALUE pairs
        #[arg(short, long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
    },
    /// List available directive names
    Schedule {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// List registered actions
    Actions,
    /// Create a new Directive scaffold
    Create {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Open a Directive YAML in $COREX_EDITOR / $EDITOR or the OS default app
    Edit {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validate a directive YAML file
    Validate {
        path: PathBuf,
        /// Require declared permissions covering all steps
        #[arg(long)]
        strict: bool,
    },
    /// Interactive REPL
    Repl,
    /// Daemon control
    Daemon {
        #[command(subcommand)]
        command: DaemonCmd,
    },
    /// File watch supervisor (PM2-style)
    Watch {
        #[command(subcommand)]
        command: watch_cmd::WatchCommands,
    },
    /// Cron scheduler supervisor
    Cron {
        #[command(subcommand)]
        command: cron_cmd::CronCommands,
    },
    /// UI element probe (Windows UIAutomation)
    Ui {
        #[command(subcommand)]
        command: ui_cmd::UiCommands,
    },
}

#[derive(Subcommand, Debug)]
enum DaemonCmd {
    /// Start corex-daemon in the background
    Start,
    /// Stop a running daemon
    Stop,
    /// Show daemon status
    Status,
    /// Run daemon in the foreground
    Run,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Commands::Run { target, inputs } => cmd_run(&target, &inputs, cli.dir.as_deref()).await,
        Commands::Schedule { dir } => cmd_schedule(dir.or(cli.dir).as_deref()),
        Commands::Actions => cmd_actions(),
        Commands::Create { name, dir } => cmd_create(&name, dir.or(cli.dir).as_deref()),
        Commands::Edit { name, dir } => cmd_edit(&name, dir.or(cli.dir).as_deref()),
        Commands::Validate { path, strict } => cmd_validate(&path, strict),
        Commands::Repl => repl::run(cli.dir).await,
        Commands::Watch { command } => watch_cmd::run(command, cli.dir.as_deref()).await,
        Commands::Cron { command } => cron_cmd::run(command, cli.dir.as_deref()).await,
        Commands::Daemon { command } => cmd_daemon(command).await,
        Commands::Ui { command } => {
            let data = platform_data_dir()?;
            ui_cmd::run(command, &data).await
        }
    }
}

fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let timer = tracing_subscriber::fmt::time::ChronoLocal::new(
        "%Y-%m-%d %H:%M:%S%.3f".to_string(),
    );
    let _ = tracing_subscriber::fmt()
        .with_timer(timer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .try_init();
}

fn directives_dir(override_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(d) = override_dir {
        return Ok(d.to_path_buf());
    }
    let d = platform_data_dir()?.join("directives");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn ipc_endpoint() -> Result<PathBuf> {
    let data = platform_data_dir()?;
    let config = load_runtime_config();
    if let Some(p) = &config.daemon.socket_path {
        return Ok(resolve_data_relative(&data, p));
    }
    Ok(platform_endpoint(&data))
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

/// Auth token for daemon IPC: `COREX_TOKEN` env, else `<data_dir>/token`.
fn load_auth_token() -> Result<String> {
    if let Ok(t) = std::env::var("COREX_TOKEN") {
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let path = platform_data_dir()?.join("token");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("无法读取 auth token {}", path.display()))?;
    let token = text.trim().to_string();
    if token.is_empty() {
        bail!("auth token 为空: {}", path.display());
    }
    Ok(token)
}

fn build_registry() -> ActionRegistry {
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    let config = load_runtime_config();
    reg.apply_runtime_config(&config);
    reg
}

pub(crate) fn load_runtime_config() -> RuntimeConfig {
    let candidates = [
        PathBuf::from(RUNTIME_CONFIG),
        platform_data_dir()
            .map(|d| d.join("config.toml"))
            .unwrap_or_default(),
    ];
    for path in candidates {
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                // Accept either flat RuntimeConfig or nested [runtime]/[plugins]
                if let Ok(cfg) = toml::from_str::<RuntimeConfigWrapper>(&text) {
                    return cfg.into_runtime();
                }
            }
        }
    }
    RuntimeConfig::default()
}

#[derive(Debug, serde::Deserialize, Default)]
struct RuntimeConfigWrapper {
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
    filesystem_roots: Option<Vec<PathBuf>>,
    #[serde(default)]
    ui_profile: Option<String>,
    #[serde(default)]
    ui_max_selector_chain: Option<usize>,
    #[serde(default)]
    ui_max_settle_ms: Option<u64>,
}

impl RuntimeConfigWrapper {
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

fn parse_inputs(pairs: &[String]) -> Result<HashMap<String, Value>> {
    let mut map = HashMap::new();
    for p in pairs {
        let (k, v) = p
            .split_once('=')
            .with_context(|| format!("输入格式应为 KEY=VALUE: {p}"))?;
        map.insert(k.to_string(), Value::from_cli_literal(v));
    }
    Ok(map)
}

fn resolve_directive_path(target: &str, dir: Option<&Path>) -> Result<PathBuf> {
    let as_path = PathBuf::from(target);
    if as_path.exists() {
        return Ok(as_path);
    }
    let base = directives_dir(dir)?;
    let candidates = [
        base.join(format!("{target}.yaml")),
        base.join(format!("{target}.yml")),
        PathBuf::from("examples/directives").join(format!("{target}.yaml")),
        PathBuf::from("examples/directives").join(format!("{target}.yml")),
    ];
    for c in candidates {
        if c.exists() {
            return Ok(c);
        }
    }
    bail!("指令未找到: {target}");
}

pub(crate) async fn cmd_run(target: &str, inputs: &[String], dir: Option<&Path>) -> Result<()> {
    let path = resolve_directive_path(target, dir)?;
    let directive = Directive::from_yaml_file(&path)?;
    let input = parse_inputs(inputs)?;
    let config = load_runtime_config();
    let ctx = ExecutionContext::new(config.clone()).with_input(input);

    let registry = Arc::new(build_registry());
    let mut pipeline = Pipeline::new(registry);
    if config.history.enabled {
        let hist_path = if config.history.file.is_absolute() {
            config.history.file.clone()
        } else {
            platform_data_dir()?.join(&config.history.file)
        };
        let history = ExecutionHistory::open(hist_path).context("无法打开执行历史")?;
        pipeline = pipeline.with_history(history);
    }
    {
        let audit_path = platform_data_dir()?.join("audit.jsonl");
        if let Ok(audit) = ExecutionAudit::open(audit_path) {
            pipeline = pipeline.with_audit(audit);
        }
    }
    let result = pipeline.execute(&directive, ctx).await?;
    println!("{}", serde_json::to_string_pretty(&result.to_json())?);
    Ok(())
}

pub(crate) fn cmd_schedule(dir: Option<&Path>) -> Result<()> {
    let base = directives_dir(dir)?;
    let mut names = Vec::new();
    if base.exists() {
        for entry in std::fs::read_dir(&base)? {
            let entry = entry?;
            let path = entry.path();
            if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yaml") | Some("yml")
            ) {
                if let Some(stem) = path.file_stem() {
                    names.push(stem.to_string_lossy().to_string());
                }
            }
        }
    }
    // Also list examples
    let examples = PathBuf::from("examples/directives");
    if examples.exists() {
        for entry in std::fs::read_dir(&examples)? {
            let entry = entry?;
            let path = entry.path();
            if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yaml") | Some("yml")
            ) {
                if let Some(stem) = path.file_stem() {
                    let name = stem.to_string_lossy().to_string();
                    if !names.contains(&name) {
                        names.push(format!("{name} (examples)"));
                    }
                }
            }
        }
    }
    names.sort();
    if names.is_empty() {
        println!("(无指令)");
    } else {
        for n in names {
            println!("{n}");
        }
    }
    Ok(())
}

pub(crate) fn cmd_actions() -> Result<()> {
    let reg = build_registry();
    for meta in reg.list() {
        println!(
            "{:<24} [{}] {}",
            meta.id,
            format!("{:?}", meta.category).to_lowercase(),
            meta.description
        );
    }
    Ok(())
}

fn cmd_create(name: &str, dir: Option<&Path>) -> Result<()> {
    let base = directives_dir(dir)?;
    let path = base.join(format!("{name}.yaml"));
    if path.exists() {
        bail!("已存在: {}", path.display());
    }
    let scaffold = format!(
        r#"name: {name}
description: ""
inputs: []
variables: {{}}
steps:
  - id: hello
    action: template.render
    params:
      template: "Hello from {name}"
    save_to: message
"#
    );
    std::fs::write(&path, scaffold)?;
    println!("已创建 {}", path.display());
    Ok(())
}

pub(crate) fn cmd_edit(name: &str, dir: Option<&Path>) -> Result<()> {
    let path = resolve_directive_path(name, dir)?;
    editor::open_in_editor(&path)?;
    println!("已打开 {}", path.display());
    Ok(())
}

fn cmd_validate(path: &Path, strict: bool) -> Result<()> {
    let directive = Directive::from_yaml_file(path)?;
    let reg = build_registry();
    let mut missing = Vec::new();
    fn walk(steps: &[corex_engine::Step], reg: &ActionRegistry, missing: &mut Vec<String>) {
        use corex_engine::Step;
        for s in steps {
            match s {
                Step::Action(a) => {
                    if !reg.contains(&a.action) {
                        missing.push(format!("{} ({})", a.action, a.id));
                    }
                }
                Step::If(i) => {
                    walk(&i.then, reg, missing);
                    walk(&i.else_steps, reg, missing);
                }
                Step::Repeat(r) => walk(&r.steps, reg, missing),
                Step::Parallel(p) => walk(&p.parallel, reg, missing),
            }
        }
    }
    walk(&directive.steps, &reg, &mut missing);
    if !missing.is_empty() {
        bail!("unregistered actions: {}", missing.join(", "));
    }
    if strict {
        validate_permissions(&directive).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    println!("OK: {} ({} steps)", directive.name, directive.steps.len());
    Ok(())
}
async fn cmd_daemon(cmd: DaemonCmd) -> Result<()> {
    match cmd {
        DaemonCmd::Run => {
            // Foreground: exec corex-daemon if on PATH, else remind user.
            let status = Command::new("corex-daemon")
                .status()
                .context("无法启动 corex-daemon，请确认已安装并在 PATH 中")?;
            if !status.success() {
                bail!("corex-daemon 退出码: {:?}", status.code());
            }
            Ok(())
        }
        DaemonCmd::Start => {
            let _child = Command::new("corex-daemon")
                .spawn()
                .context("无法后台启动 corex-daemon")?;
            println!("已请求启动 corex-daemon");
            Ok(())
        }
        DaemonCmd::Stop => {
            let endpoint = ipc_endpoint()?;
            let token = load_auth_token()?;
            let mut transport = platform_transport(&endpoint);
            let req = Request::Shutdown {
                id: 1,
                auth_token: None,
            }
            .with_auth_token(token);
            match transport.send(&req).await {
                Ok(Response::Bye { .. }) | Ok(Response::Ok { .. }) => {
                    println!("已发送 shutdown");
                    Ok(())
                }
                Ok(Response::Error { error, .. }) => bail!("shutdown 失败: {error}"),
                Ok(_) => Ok(()),
                Err(e) => bail!("daemon 未运行或无法连接: {e}"),
            }
        }
        DaemonCmd::Status => {
            let endpoint = ipc_endpoint()?;
            let token = match load_auth_token() {
                Ok(t) => t,
                Err(_) => {
                    println!("stopped");
                    return Ok(());
                }
            };
            let mut transport = platform_transport(&endpoint);
            let req = Request::Ping {
                id: 1,
                auth_token: None,
            }
            .with_auth_token(token);
            match transport.send(&req).await {
                Ok(Response::Pong { .. }) => {
                    println!("running ({})", endpoint.display());
                    Ok(())
                }
                Ok(other) => {
                    println!("unexpected: {other:?}");
                    Ok(())
                }
                Err(_) => {
                    println!("stopped");
                    Ok(())
                }
            }
        }
    }
}
