//! `corex cron` subcommands.

use crate::trigger_cmd;
use anyhow::Result;
use clap::Subcommand;
use corex_engine::JobKind;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum CronCommands {
    /// Start cron supervisor in background
    Start {
        name: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// List running cron jobs (use NAME column for commands)
    Ps,
    /// Enter job: tail realtime log (Ctrl+C detach, does not stop supervisor)
    Attach {
        /// Directive name
        name: String,
    },
    /// Tail supervisor log
    Logs {
        name: Option<String>,
        #[arg(long, default_value_t = 50)]
        lines: usize,
        #[arg(short, long)]
        follow: bool,
    },
    /// Send control message: run-now | status | stop
    Send {
        name: String,
        msg: String,
    },
    /// Stop a cron job
    Stop {
        name: String,
    },
    /// Restart a cron job
    Restart {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Attach if running; else prompt to start (use --foreground for dev mode)
    Run {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        foreground: bool,
        #[arg(long, hide = true)]
        supervised: bool,
        #[arg(long, hide = true)]
        job_id: Option<String>,
    },
}

pub async fn run(cmd: CronCommands, global_dir: Option<&std::path::Path>) -> Result<()> {
    match cmd {
        CronCommands::Start { name, all, dir } => {
            let dir = dir.as_deref().or(global_dir);
            if all {
                trigger_cmd::start_all(JobKind::Cron, dir).await
            } else if let Some(n) = name {
                trigger_cmd::start_job(JobKind::Cron, &n, dir).await
            } else {
                anyhow::bail!("需要指令名或 --all");
            }
        }
        CronCommands::Ps => trigger_cmd::cmd_ps(JobKind::Cron),
        CronCommands::Attach { name } => trigger_cmd::cmd_attach(JobKind::Cron, &name).await,
        CronCommands::Logs { name, lines, follow } => {
            trigger_cmd::cmd_logs(JobKind::Cron, name.as_deref(), lines, follow).await
        }
        CronCommands::Send { name, msg } => trigger_cmd::cmd_send(JobKind::Cron, &name, &msg),
        CronCommands::Stop { name } => trigger_cmd::cmd_stop(JobKind::Cron, &name),
        CronCommands::Restart { name, dir } => {
            let dir = dir.as_deref().or(global_dir);
            trigger_cmd::cmd_restart(JobKind::Cron, &name, dir).await
        }
        CronCommands::Run {
            name,
            dir,
            foreground,
            supervised,
            job_id,
        } => {
            let dir = dir.as_deref().or(global_dir);
            if supervised {
                trigger_cmd::cmd_run_supervised(
                    JobKind::Cron,
                    job_id.as_deref().unwrap_or(&name),
                    dir,
                )
                .await
            } else {
                trigger_cmd::cmd_run_interactive(JobKind::Cron, &name, dir, foreground).await
            }
        }
    }
}
