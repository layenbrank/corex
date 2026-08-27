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
    /// Show running watch jobs
    Ps,
    /// Tail supervisor log (placeholder: shows status)
    Attach {
        id: String,
    },
    /// Send control message: RUN_NOW | STATUS | STOP
    Send {
        id: String,
        msg: String,
    },
    /// Stop a watch job
    Stop {
        id: String,
    },
    /// Restart a watch job
    Restart {
        id: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Run watch in foreground
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
        WatchCommands::Attach { id } => {
            println!("attach `{id}` — 日志请查看 <data>/watch/{id}/");
            Ok(())
        }
        WatchCommands::Send { id, msg } => trigger_cmd::cmd_send(JobKind::Watch, &id, &msg),
        WatchCommands::Stop { id } => trigger_cmd::cmd_stop(JobKind::Watch, &id),
        WatchCommands::Restart { id, dir } => {
            let dir = dir.as_deref().or(global_dir);
            let _ = trigger_cmd::cmd_stop(JobKind::Watch, &id);
            trigger_cmd::start_job(JobKind::Watch, &id, dir).await
        }
        WatchCommands::Run {
            name,
            dir,
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
                trigger_cmd::cmd_run_foreground(JobKind::Watch, &name, dir).await
            }
        }
    }
}
