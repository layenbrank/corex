//! Corex CLI 入口。

mod cli;
mod cron;
mod editor;
mod exit;
mod output;
mod repl;
mod scheduler;
mod settings;
mod ui;
mod update;
mod watch;

use crate::cli::{Cli, Commands, DaemonCmd};
use crate::output::{errln, outln};
use crate::scheduler::Paths;
use anyhow::{Context, Result, bail};
use clap::Parser;
use corex_core::{ExecutionContext, Value};
use corex_engine::{Directive, ExecutionAudit, ExecutionHistory, Pipeline, validate_permissions};
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{Transport, config_paths, data_dir, ipc_connect, ipc_endpoint};
use corex_registry::ActionRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // 配置在任何分派之前只读一次：文件存在却解析失败必须让整次运行失败，
    // 而不是把默认值悄悄糊到命令的一部分上。
    let paths = match &cli.config {
        Some(path) => vec![path.clone()],
        None => config_paths(),
    };
    // 显式指定的文件必须存在：因为路径敲错而跑在默认值上，正是读取层已经戒掉的那种“安静回退”。
    let loaded = match &cli.config {
        Some(path) if !path.is_file() => Err(corex_core::EngineError::Config(format!(
            "配置文件不存在: {}",
            path.display()
        ))),
        _ => settings::init(&paths),
    };
    match loaded {
        Ok(warnings) => {
            for issue in &warnings {
                errln!("配置警告 [{}]: {}", issue.key, issue.message);
            }
        }
        Err(err) => {
            let err = anyhow::Error::new(err);
            errln!("错误: {err:?}");
            return ExitCode::from(exit::ExitStatus::read(&err).code());
        }
    }

    // 日志在配置读完*之后*才初始化，这样 `[logging] level` 才能作为 `-v` 与 `COREX_LOG`
    // 覆盖的默认值；更早初始化会让这个键对 CLI 静默失效。
    init_tracing(cli.verbose);

    // 在分派前启动，让慢请求与命令重叠；在命令之后 join，
    // 保证提示不会改变命令自身的输出与退出码。
    let notice = update::start_notice(cli.command.wants_update_notice());
    let result = dispatch(cli).await;
    update::finish_notice(notice).await;

    // 所有退出码都由 `ExitStatus` 给出，每个结果的数字只定义在一处。唯一不能影响它的是
    // stdout 被关闭：撞上断管的写入已被 `output` 吞掉，所以这里的 `Err` 即使读方提前离开
    // （`| head`）也是真失败。
    let status = match result {
        Ok(()) => exit::ExitStatus::Success,
        Err(err) => {
            errln!("错误: {err:?}");
            exit::ExitStatus::read(&err)
        }
    };
    ExitCode::from(status.code())
}

/// 把解析好的命令送到对应实现。
async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Run { target, inputs } => cmd_run(&target, &inputs, cli.dir.as_deref()).await,
        Commands::Schedule { dir } => cmd_schedule(dir.or(cli.dir).as_deref()),
        Commands::Actions => cmd_actions(),
        Commands::Create { name, dir } => cmd_create(&name, dir.or(cli.dir).as_deref()),
        Commands::Edit { name, dir } => cmd_edit(&name, dir.or(cli.dir).as_deref()),
        Commands::Validate { path, strict } => cmd_validate(path.as_deref(), strict),
        Commands::Repl => repl::run(cli.dir).await,
        Commands::Watch { command } => watch::run(command, cli.dir.as_deref()).await,
        Commands::Cron { command } => cron::run(command, cli.dir.as_deref()).await,
        Commands::Daemon { command } => cmd_daemon(command).await,
        Commands::Ui { command } => {
            let data = data_dir()?;
            ui::run(command, &data).await
        }
        Commands::Update { args } => update::run(args).await,
    }
}

fn init_tracing(verbose: u8) {
    // 没有 `-v` 时由配置决定；`COREX_LOG` 仍然压过两者，因为下面的 env filter 会更早被查询。
    let configured = settings::effective().logging.level.clone();
    let level = match verbose {
        0 => configured.as_str(),
        1 => "debug",
        _ => "trace",
    };
    let timer =
        tracing_subscriber::fmt::time::ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string());
    let _ = tracing_subscriber::fmt()
        .with_timer(timer)
        // 用 stderr，绝不用 stdout。`fmt()` 默认写 stdout，于是任何 info 级事件都会
        // 落在命令结果中间——`corex run x --json | jq` 就会看到它——而且读方提前关闭
        // stdout（`| head`）时连日志写入者一起弄坏。
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .try_init();
}

fn resolve_endpoint() -> Result<PathBuf> {
    let data = data_dir()?;
    let config = settings::effective();
    if let Some(p) = &config.daemon.socket_path {
        return Ok(resolve_data_relative(&data, p));
    }
    Ok(ipc_endpoint(&data))
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

/// 守护进程 IPC 的鉴权 token：先用 `COREX_TOKEN` 环境变量，否则读 `<data_dir>/token`。
fn auth_token() -> Result<String> {
    if let Ok(t) = std::env::var("COREX_TOKEN")
        && !t.is_empty()
    {
        return Ok(t);
    }
    let path = data_dir()?.join("token");
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
    reg.remove_disabled(&settings::effective().plugins);
    reg
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

pub(crate) async fn cmd_run(target: &str, inputs: &[String], dir: Option<&Path>) -> Result<()> {
    let path = Paths::resolve(target, dir)?;
    let directive = Directive::from_yaml_file(&path)?;
    let input = parse_inputs(inputs)?;
    let config = settings::effective().clone();
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
    {
        let audit_path = data_dir()?.join("audit.jsonl");
        let audit_display = audit_path.display().to_string();
        match ExecutionAudit::open(audit_path) {
            Ok(audit) => pipeline = pipeline.with_audit(audit),
            // 刻意只做尽力而为，但绝不静默：用户以为已记录的运行，
            // 不能一声不喘地没被记录。
            Err(err) => errln!("警告: 无法打开审计日志 {audit_display}（{err}）"),
        }
    }
    let result = pipeline.execute(&directive, ctx).await?;
    outln!("{}", serde_json::to_string_pretty(&result.to_json())?);
    Ok(())
}

pub(crate) fn cmd_schedule(dir: Option<&Path>) -> Result<()> {
    let base = Paths::dir(dir)?;
    let mut names = Vec::new();
    if base.exists() {
        for entry in std::fs::read_dir(&base)? {
            let entry = entry?;
            let path = entry.path();
            if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yaml") | Some("yml")
            ) && let Some(stem) = path.file_stem()
            {
                names.push(stem.to_string_lossy().to_string());
            }
        }
    }
    // 顺带列出 examples 里的指令
    let examples = PathBuf::from("examples/directives");
    if examples.exists() {
        for entry in std::fs::read_dir(&examples)? {
            let entry = entry?;
            let path = entry.path();
            if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yaml") | Some("yml")
            ) && let Some(stem) = path.file_stem()
            {
                let name = stem.to_string_lossy().to_string();
                if !names.contains(&name) {
                    names.push(format!("{name} (examples)"));
                }
            }
        }
    }
    names.sort();
    if names.is_empty() {
        outln!("(无指令)");
    } else {
        for n in names {
            outln!("{n}");
        }
    }
    Ok(())
}

pub(crate) fn cmd_actions() -> Result<()> {
    let reg = build_registry();
    for meta in reg.actions() {
        outln!(
            "{:<24} [{}] {}",
            meta.id,
            format!("{:?}", meta.category).to_lowercase(),
            meta.description
        );
    }
    Ok(())
}

fn cmd_create(name: &str, dir: Option<&Path>) -> Result<()> {
    let base = Paths::dir(dir)?;
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
    outln!("已创建 {}", path.display());
    Ok(())
}

pub(crate) fn cmd_edit(name: &str, dir: Option<&Path>) -> Result<()> {
    let path = Paths::resolve(name, dir)?;
    editor::open_in_editor(&path)?;
    outln!("已打开 {}", path.display());
    Ok(())
}

/// 校验指令；不给路径时校验配置。
///
/// 配置分支直接用 `settings` 已经读到的结果，不重新解析文件，
/// 这样报出的就是本进程实际用的那份。
fn cmd_validate(path: Option<&Path>, strict: bool) -> Result<()> {
    let Some(path) = path else {
        if strict {
            // 默默忽略它就会变成一个空转开关，正是本仓库想清掉的东西；
            // `Usage` 把它归为用法错误，而不是运行失败。
            return Err(corex_core::EngineError::Usage(
                "--strict 只用于指令校验，不适用于配置检查".into(),
            )
            .into());
        }
        return cmd_validate_config();
    };
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
        bail!("未注册的动作: {}", missing.join(", "));
    }
    if strict {
        // `validate_permissions` 只给出文字，所以需要一个带类型的载体——
        // 而门禁拒绝就是权限错误，退出码表已经认得它。
        validate_permissions(&reg, &directive)
            .map_err(|e| anyhow::Error::new(corex_core::ActionError::PermissionDenied(e)))?;
    }
    outln!(
        "校验通过: {}（{} 步）",
        directive.name,
        directive.steps.len()
    );
    Ok(())
}

/// 报告生效配置以及其中的问题。
fn cmd_validate_config() -> Result<()> {
    match settings::source() {
        Some(path) => outln!("配置文件: {}", path.display()),
        None => outln!("未找到配置文件，使用默认值"),
    }
    let issues = corex_core::config::validate(settings::effective());
    if issues.is_empty() {
        outln!("配置有效");
        return Ok(());
    }
    // 这些问题本身在启动阶段已经作为告警报过；脚本在这里需要的是退出状态。
    // `EngineError::Config` 对应 usage 码，所以配置损坏与运行失败是可区分的。
    Err(corex_core::EngineError::Config(format!("配置有 {} 个问题", issues.len())).into())
}
/// 以后台方式启动 `corex-daemon`，并给它自己的日志文件。
///
/// 守护进程的日志流按设计就是它的 stdout，因此绝不能继承 CLI 的 stdout：这里的 stdout
/// 是结果通道，而且可能是管道或被重定向的文件。
fn start_daemon() -> Result<()> {
    let log_path = data_dir()?.join("daemon.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context("无法打开 daemon 日志文件")?;
    let log_err = log.try_clone().context("无法复制 daemon 日志句柄")?;
    let _child = Command::new("corex-daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()
        .context("无法后台启动 corex-daemon")?;
    outln!("已请求启动 corex-daemon（日志: {}）", log_path.display());
    Ok(())
}

async fn cmd_daemon(cmd: DaemonCmd) -> Result<()> {
    match cmd {
        DaemonCmd::Run => {
            // 前台：PATH 里有 corex-daemon 就直接跑，否则提醒用户。
            let status = Command::new("corex-daemon")
                .status()
                .context("无法启动 corex-daemon，请确认已安装并在 PATH 中")?;
            if !status.success() {
                bail!("corex-daemon 退出码: {:?}", status.code());
            }
            Ok(())
        }
        DaemonCmd::Start => start_daemon(),
        DaemonCmd::Stop => {
            let endpoint = resolve_endpoint()?;
            let token = auth_token()?;
            let mut transport = ipc_connect(&endpoint);
            let req = Request::Shutdown {
                id: 1,
                auth_token: None,
            }
            .with_auth_token(token);
            match transport.send(&req).await {
                Ok(Response::Bye { .. }) | Ok(Response::Ok { .. }) => {
                    outln!("已发送 shutdown");
                    Ok(())
                }
                Ok(Response::Error { error, .. }) => bail!("shutdown 失败: {error}"),
                Ok(_) => Ok(()),
                Err(e) => bail!("daemon 未运行或无法连接: {e}"),
            }
        }
        DaemonCmd::Status => {
            let endpoint = resolve_endpoint()?;
            let token = match auth_token() {
                Ok(t) => t,
                Err(err) => {
                    // token 文件缺失是正常的“从未启动过”，但真的读取失败会和它长得一样。
                    errln!("提示: 未读到 auth token（{err}），按未运行处理");
                    outln!("已停止");
                    return Ok(());
                }
            };
            let mut transport = ipc_connect(&endpoint);
            let req = Request::Ping {
                id: 1,
                auth_token: None,
            }
            .with_auth_token(token);
            match transport.send(&req).await {
                Ok(Response::Pong { .. }) => {
                    outln!("运行中 ({})", endpoint.display());
                    Ok(())
                }
                Ok(other) => {
                    outln!("意外状态: {other:?}");
                    Ok(())
                }
                Err(_) => {
                    outln!("已停止");
                    Ok(())
                }
            }
        }
    }
}
