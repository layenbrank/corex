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
    /// Show running cron jobs
    Ps,
    /// Send control message: RUN_NOW | STATUS | STOP
    Send {
        id: String,
        msg: String,
    },
    /// Stop a cron job
    Stop {
        id: String,
    },
    /// Restart a cron job
    Restart {
        id: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Run cron in foreground
    Run {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
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
        CronCommands::Send { id, msg } => trigger_cmd::cmd_send(JobKind::Cron, &id, &msg),
        CronCommands::Stop { id } => trigger_cmd::cmd_stop(JobKind::Cron, &id),
        CronCommands::Restart { id, dir } => {
            let dir = dir.as_deref().or(global_dir);
            let _ = trigger_cmd::cmd_stop(JobKind::Cron, &id);
            trigger_cmd::start_job(JobKind::Cron, &id, dir).await
        }
        CronCommands::Run {
            name,
            dir,
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
                trigger_cmd::cmd_run_foreground(JobKind::Cron, &name, dir).await
            }
        }
    }
}
