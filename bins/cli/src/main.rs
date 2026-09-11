//! Corex CLI 入口。

mod actions;
mod ask;
mod cli;
mod create;
mod cron;
mod doctor;
mod editor;
mod exit;
mod fuzzy;
mod output;
mod progress;
mod repl;
mod run;
mod scheduler;
mod schema;
mod settings;
mod steps;
mod ui;
mod update;
mod validate;
mod watch;

use crate::cli::{Cli, Commands, DaemonCmd};
use crate::output::{errln, outln};
use crate::scheduler::Paths;
use anyhow::{Context, Result, bail};
use clap::Parser;
use corex_core::Value;
use corex_ipc::protocol::{Request, Response};
use corex_ipc::{Transport, config_paths, data_dir, ipc_connect};
use corex_registry::ActionRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

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
///
/// `corex repl` 也走这里：REPL 里的每一行都会被重新拼成 `corex ...` 再进来，
/// 因此命令行与 REPL 永远共用同一套行为。
pub(crate) async fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Run {
            target,
            inputs,
            dry_run,
            json_events,
            quiet,
            yes,
            remote,
        } => {
            let options = run::Options {
                inputs,
                dry_run,
                json_events,
                quiet,
                yes,
                remote,
            };
            run::cmd_run(target.as_deref(), &options, cli.dir.as_deref()).await
        }
        Commands::Schedule { dir } => cmd_schedule(dir.or(cli.dir).as_deref()),
        Commands::Actions { id } => actions::cmd_actions(id.as_deref()),
        Commands::Create {
            name,
            template,
            force,
            dir,
        } => create::cmd_create(
            name.as_deref(),
            template.as_deref(),
            force,
            dir.or(cli.dir).as_deref(),
        ),
        Commands::Edit { name, dir } => cmd_edit(&name, dir.or(cli.dir).as_deref()),
        Commands::Validate {
            path,
            strict,
            watch,
        } => validate::cmd_validate(path.as_deref(), strict, watch).await,
        Commands::Schema { write } => schema::cmd_schema(write.as_deref()),
        Commands::Completions { shell } => cmd_completions(shell),
        Commands::Doctor => doctor::cmd_doctor().await,
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
    // CLI 的默认级别是 `warn`，**不是**配置里的 `[logging].level`。
    //
    // 理由是同一条事实不该在两个通道里各说一遍：CLI 已经有专门的进度通道（stderr 上的
    // ✓ 行 + spinner）与结果通道（stdout），而引擎还会在同样的位置用日志格式再说一遍
    // （`corex_engine::audit` 在每个步骤前后各一条 info）。两个通道讲同一件事的结果，
    // 就是 `corex run` 的进度与结果被十几行 `INFO corex_engine::audit:` 埋掉。
    //
    // 想要日志时它是随手可得的：`-v` 开 debug、`-vv` 开 trace，`COREX_LOG` 压过两者
    // （下面的 env filter 会更早被查询）。
    //
    // `[logging]` 因此只对守护进程生效——那里没有进度通道，日志流就是它的输出。
    let level = match verbose {
        0 => "warn",
        1 => "debug",
        _ => "trace",
    };
    let timer =
        tracing_subscriber::fmt::time::ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string());
    let _ = tracing_subscriber::fmt()
        .with_timer(timer)
        // 用 stderr，绝不用 stdout。`fmt()` 默认写 stdout，于是任何 info 级事件都会
        // 落在命令结果中间——`corex run x --json-events | jq` 就会看到它——而且读方提前关闭
        // stdout（`| head`）时连日志写入者一起弄坏。
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .try_init();
}

/// 本次运行实际使用的 IPC 端点：配置里没写 `socket_path` 时就是平台默认端点。
///
/// 端点无效属于配置问题，所以走 `EngineError::Config`（退出码 2），
/// 与「配置损坏」保持一致，而不是退成通用失败。
pub(crate) fn resolve_endpoint() -> Result<PathBuf> {
    let data = data_dir()?;
    let configured = settings::effective().daemon.socket_path.clone();
    corex_ipc::resolve_endpoint(&data, configured.as_deref())
        .map_err(|e| anyhow::Error::new(corex_core::EngineError::Config(e.to_string())))
}

/// 守护进程 IPC 的鉴权 token：先用 `COREX_TOKEN` 环境变量，否则读 `<data_dir>/token`。
pub(crate) fn auth_token() -> Result<String> {
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

pub(crate) fn build_registry() -> ActionRegistry {
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    reg.remove_disabled(&settings::effective().plugins);
    reg
}

/// CLI 自己的用法失误：走 `EngineError::Usage` 才对应退出码 2，而不是通用失败 1。
pub(crate) fn usage(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(corex_core::EngineError::Usage(message.into()))
}

pub(crate) fn parse_inputs(pairs: &[String]) -> Result<HashMap<String, Value>> {
    let mut map = HashMap::new();
    for p in pairs {
        let (k, v) = p
            .split_once('=')
            .with_context(|| format!("输入格式应为 KEY=VALUE: {p}"))?;
        map.insert(k.to_string(), Value::from_cli_literal(v));
    }
    Ok(map)
}

/// 列出可用指令名。
///
/// 自有指令与 `examples/directives` 里的演示一起列，后者带 `(examples)` 后缀；
/// 枚举与选单共用 [`Paths::names`]，两处不会走偏。
pub(crate) fn cmd_schedule(dir: Option<&Path>) -> Result<()> {
    let named = Paths::names(dir)?;
    if named.is_empty() {
        outln!("(无指令)");
        return Ok(());
    }
    for entry in named {
        outln!("{}", entry.label());
    }
    Ok(())
}

pub(crate) fn cmd_edit(name: &str, dir: Option<&Path>) -> Result<()> {
    let path = Paths::resolve(name, dir)?;
    editor::open_in_editor(&path)?;
    outln!("已打开 {}", path.display());
    Ok(())
}

/// 打印某个 shell 的补全脚本。
///
/// 走 `output::bytes` 而不是 `outln!`：生成的是脚本，不是一行文本，而 shell 补全
/// 最常见的用法就是 `corex completions powershell | Out-File ...`——这条管道的读方随时会离开。
fn cmd_completions(shell: clap_complete::Shell) -> Result<()> {
    let mut script = Vec::new();
    let mut command = <Cli as clap::CommandFactory>::command();
    clap_complete::generate(shell, &mut command, "corex", &mut script);
    output::bytes(&script)?;
    Ok(())
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
            outln!("{}", daemon_state().await?);
            Ok(())
        }
    }
}

/// 守护进程当前状态的一行结论；`corex daemon status` 与 `corex doctor` 共用。
pub(crate) async fn daemon_state() -> Result<String> {
    let endpoint = resolve_endpoint()?;
    let token = match auth_token() {
        Ok(token) => token,
        Err(err) => {
            // token 文件缺失是正常的“从未启动过”，但真的读取失败会和它长得一样。
            // 结论一样是「没在跑」，细节留给 `-v`。
            tracing::debug!(error = %err, "未读到 auth token，按未运行处理");
            return Ok("已停止".to_string());
        }
    };
    let mut transport = ipc_connect(&endpoint);
    let req = Request::Ping {
        id: 1,
        auth_token: None,
    }
    .with_auth_token(token);
    Ok(match transport.send(&req).await {
        Ok(Response::Pong { .. }) => format!("运行中 ({})", endpoint.display()),
        Ok(other) => format!("意外状态: {other:?}"),
        Err(_) => "已停止".to_string(),
    })
}
