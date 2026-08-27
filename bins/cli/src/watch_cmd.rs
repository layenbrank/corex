//! `corex watch` subcommands.

use crate::trigger_cmd;
use anyhow::Result;
use clap::Subcommand;
use corex_engine::JobKind;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum WatchCommands {
    /// Start watch supervisor in background
    Start {
        /// Directive name (omit with --all)
        name: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// List running watch jobs (use NAME column for commands)
    Ps,
    /// Enter job: tail realtime log (Ctrl+C detach, does not stop supervisor)
    Attach {
        /// Directive name (e.g. build-client)
        name: String,
    },
    /// Tail supervisor log
    Logs {
        /// Directive name; omit to list log paths
        name: Option<String>,
        #[arg(long, default_value_t = 50)]
        lines: usize,
        #[arg(short, long)]
        follow: bool,
    },
    /// Send control message: run-now | status | stop
    Send {
        /// Directive name
        name: String,
        msg: String,
    },
    /// Stop a watch job
    Stop {
        /// Directive name
        name: String,
    },
    /// Restart a watch job
    Restart {
        /// Directive name
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Attach if running; else prompt to start (use --foreground for dev mode)
    Run {
        /// Directive name
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Foreground dev mode (Ctrl+C stops supervisor)
        #[arg(long)]
        foreground: bool,
        #[arg(long, hide = true)]
        supervised: bool,
        #[arg(long, hide = true)]
        job_id: Option<String>,
    },
}

pub async fn run(cmd: WatchCommands, global_dir: Option<&std::path::Path>) -> Result<()> {
    match cmd {
        WatchCommands::Start { name, all, dir } => {
            let dir = dir.as_deref().or(global_dir);
            if all {
                trigger_cmd::start_all(JobKind::Watch, dir).await
            } else if let Some(n) = name {
                trigger_cmd::start_job(JobKind::Watch, &n, dir).await
            } else {
                anyhow::bail!("需要指令名或 --all");
            }
        }
        WatchCommands::Ps => trigger_cmd::cmd_ps(JobKind::Watch),
        WatchCommands::Attach { name } => trigger_cmd::cmd_attach(JobKind::Watch, &name).await,
        WatchCommands::Logs { name, lines, follow } => {
            trigger_cmd::cmd_logs(JobKind::Watch, name.as_deref(), lines, follow).await
        }
        WatchCommands::Send { name, msg } => trigger_cmd::cmd_send(JobKind::Watch, &name, &msg),
        WatchCommands::Stop { name } => trigger_cmd::cmd_stop(JobKind::Watch, &name),
        WatchCommands::Restart { name, dir } => {
            let dir = dir.as_deref().or(global_dir);
            trigger_cmd::cmd_restart(JobKind::Watch, &name, dir).await
        }
        WatchCommands::Run {
            name,
            dir,
            foreground,
            supervised,
            job_id,
        } => {
            let dir = dir.as_deref().or(global_dir);
            if supervised {
                trigger_cmd::cmd_run_supervised(
                    JobKind::Watch,
                    job_id.as_deref().unwrap_or(&name),
                    dir,
                )
                .await
            } else {
                trigger_cmd::cmd_run_interactive(JobKind::Watch, &name, dir, foreground).await
            }
        }
    }
}
