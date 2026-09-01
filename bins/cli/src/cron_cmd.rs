//! `corex cron` subcommands.

use crate::trigger_cmd;
use anyhow::Result;
use clap::Subcommand;
use corex_engine::JobKind;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum CronCommands {
    /// Run cron supervisor (background by default; use --foreground for dev)
    Run {
        /// Directive name (omit with --all)
        name: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Foreground dev mode in current terminal (Ctrl+C stops supervisor)
        #[arg(long)]
        foreground: bool,
        #[arg(long, hide = true)]
        supervised: bool,
        #[arg(long, hide = true)]
        job_id: Option<String>,
    },
    /// List running cron jobs (use NAME column for commands)
    Ps,
    /// Tail realtime supervisor log (Ctrl+C detach, does not stop supervisor)
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
    Send { name: String, msg: String },
    /// Stop a cron job (use --force to kill in-flight builds)
    Stop {
        /// Directive name
        name: String,
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// Restart a cron job
    Restart {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

pub async fn run(cmd: CronCommands, global_dir: Option<&std::path::Path>) -> Result<()> {
    match cmd {
        CronCommands::Run {
            name,
            all,
            dir,
            foreground,
            supervised,
            job_id,
        } => {
            let dir = dir.as_deref().or(global_dir);
            trigger_cmd::cmd_run(
                JobKind::Cron,
                name,
                all,
                dir,
                foreground,
                false,
                supervised,
                job_id,
            )
            .await
        }
        CronCommands::Ps => trigger_cmd::cmd_ps(JobKind::Cron),
        CronCommands::Attach { name } => trigger_cmd::cmd_attach(JobKind::Cron, &name).await,
        CronCommands::Logs {
            name,
            lines,
            follow,
        } => trigger_cmd::cmd_logs(JobKind::Cron, name.as_deref(), lines, follow).await,
        CronCommands::Send { name, msg } => trigger_cmd::cmd_send(JobKind::Cron, &name, &msg),
        CronCommands::Stop { name, force } => {
            trigger_cmd::cmd_stop(JobKind::Cron, &name, force).await
        }
        CronCommands::Restart { name, dir } => {
            let dir = dir.as_deref().or(global_dir);
            trigger_cmd::cmd_restart(JobKind::Cron, &name, dir).await
        }
    }
}
