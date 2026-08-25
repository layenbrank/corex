//! Corex CLI entrypoint.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use corex_core::{ExecutionContext, RuntimeConfig, Value};
use corex_engine::{ExecutionHistory, Pipeline, Shortcut};
use corex_ipc::protocol::{Request, Response};
use corex_ipc::transport::{Transport, UnixSocketTransport};
use corex_registry::ActionRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "corex", version, about = "Corex — composable shortcuts & actions")]
struct Cli {
    /// Shortcut / config search directory
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
    /// Run a shortcut by name or file path
    Run {
        /// Shortcut name (without .yaml) or path to YAML
        target: String,
        /// Input as KEY=VALUE pairs
        #[arg(short, long = "input", value_name = "KEY=VALUE")]
        inputs: Vec<String>,
    },
    /// List available shortcuts
    List {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// List registered actions
    Actions,
    /// Create a new shortcut scaffold
    Create {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validate a shortcut YAML file
    Validate {
        path: PathBuf,
    },
    /// Daemon control
    Daemon {
        #[command(subcommand)]
        command: DaemonCmd,
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
        Commands::List { dir } => cmd_list(dir.or(cli.dir).as_deref()),
        Commands::Actions => cmd_actions(),
        Commands::Create { name, dir } => cmd_create(&name, dir.or(cli.dir).as_deref()),
        Commands::Validate { path } => cmd_validate(&path),
        Commands::Daemon { command } => cmd_daemon(command).await,
    }
}

fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .try_init();
}

fn data_dir() -> Result<PathBuf> {
    let base = directories::ProjectDirs::from("dev", "corex", "corex")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".corex"));
    std::fs::create_dir_all(&base)?;
    Ok(base)
}

fn shortcuts_dir(override_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(d) = override_dir {
        return Ok(d.to_path_buf());
    }
    let d = data_dir()?.join("shortcuts");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn socket_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("corex.sock"))
}

fn build_registry() -> ActionRegistry {
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    let config = load_runtime_config();
    reg.apply_runtime_config(&config);
    reg
}

fn load_runtime_config() -> RuntimeConfig {
    let candidates = [
        PathBuf::from("config/default.toml"),
        data_dir()
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
    runtime: Option<RuntimeSection>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct RuntimeSection {
    #[serde(default)]
    max_parallel: Option<usize>,
    #[serde(default)]
    step_timeout_secs: Option<u64>,
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

fn parse_inputs(pairs: &[String]) -> Result<HashMap<String, Value>> {
    let mut map = HashMap::new();
    for p in pairs {
        let (k, v) = p
            .split_once('=')
            .with_context(|| format!("输入格式应为 KEY=VALUE: {p}"))?;
        map.insert(k.to_string(), Value::Str(v.to_string()));
    }
    Ok(map)
}

fn resolve_shortcut_path(target: &str, dir: Option<&Path>) -> Result<PathBuf> {
    let as_path = PathBuf::from(target);
    if as_path.exists() {
        return Ok(as_path);
    }
    let base = shortcuts_dir(dir)?;
    let candidates = [
        base.join(format!("{target}.yaml")),
        base.join(format!("{target}.yml")),
        PathBuf::from("examples/shortcuts").join(format!("{target}.yaml")),
        PathBuf::from("examples/shortcuts").join(format!("{target}.yml")),
    ];
    for c in candidates {
        if c.exists() {
            return Ok(c);
        }
    }
    bail!("快捷指令未找到: {target}");
}

async fn cmd_run(target: &str, inputs: &[String], dir: Option<&Path>) -> Result<()> {
    let path = resolve_shortcut_path(target, dir)?;
    let shortcut = Shortcut::from_yaml_file(&path)?;
    let input = parse_inputs(inputs)?;
    let config = load_runtime_config();
    let ctx = ExecutionContext::new(config.clone()).with_input(input);

    let registry = Arc::new(build_registry());
    let mut pipeline = Pipeline::new(registry);
    if config.history.enabled {
        let hist_path = if config.history.file.is_absolute() {
            config.history.file.clone()
        } else {
            data_dir()?.join(&config.history.file)
        };
        let history = ExecutionHistory::open(hist_path).context("无法打开执行历史")?;
        pipeline = pipeline.with_history(history);
    }
    let result = pipeline.execute(&shortcut, ctx).await?;
    println!("{}", serde_json::to_string_pretty(&result.to_json())?);
    Ok(())
}

fn cmd_list(dir: Option<&Path>) -> Result<()> {
    let base = shortcuts_dir(dir)?;
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
    let examples = PathBuf::from("examples/shortcuts");
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
        println!("(无快捷指令)");
    } else {
        for n in names {
            println!("{n}");
        }
    }
    Ok(())
}

fn cmd_actions() -> Result<()> {
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
    let base = shortcuts_dir(dir)?;
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

fn cmd_validate(path: &Path) -> Result<()> {
    let shortcut = Shortcut::from_yaml_file(path)?;
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
    walk(&shortcut.steps, &reg, &mut missing);
    if missing.is_empty() {
        println!("OK: {} ({} steps)", shortcut.name, shortcut.steps.len());
        Ok(())
    } else {
        bail!("未注册的动作: {}", missing.join(", "));
    }
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
            let mut transport = UnixSocketTransport::new(socket_path()?);
            match transport
                .send(&Request::Shutdown { id: 1 })
                .await
            {
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
            let mut transport = UnixSocketTransport::new(socket_path()?);
            match transport.send(&Request::Ping { id: 1 }).await {
                Ok(Response::Pong { .. }) => {
                    println!("running ({})", socket_path()?.display());
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
