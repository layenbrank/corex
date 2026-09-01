//! `corex watch` subcommands.

use crate::trigger_cmd;
use anyhow::Result;
use clap::Subcommand;
use corex_engine::JobKind;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum WatchCommands {
    /// Run watch supervisor (background by default; use --foreground for dev)
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
        /// Run pipeline once right after watch starts
        #[arg(long)]
        immediate: bool,
        #[arg(long, hide = true)]
        supervised: bool,
        #[arg(long, hide = true)]
        job_id: Option<String>,
    },
    /// List running watch jobs (use NAME column for commands)
    Ps,
    /// Tail realtime supervisor log (Ctrl+C detach, does not stop supervisor)
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
    /// Stop a watch job (use --force to kill in-flight builds)
    Stop {
        /// Directive name
        name: String,
        /// Kill supervisor and in-flight pipeline immediately
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// Restart a watch job
    Restart {
        /// Directive name
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

pub async fn run(cmd: WatchCommands, global_dir: Option<&std::path::Path>) -> Result<()> {
    match cmd {
        WatchCommands::Run {
            name,
            all,
            dir,
            foreground,
            immediate,
            supervised,
            job_id,
        } => {
            let dir = dir.as_deref().or(global_dir);
            trigger_cmd::cmd_run(
                JobKind::Watch,
                name,
                all,
                dir,
                foreground,
                immediate,
                supervised,
                job_id,
            )
            .await
        }
        WatchCommands::Ps => trigger_cmd::cmd_ps(JobKind::Watch),
        WatchCommands::Attach { name } => trigger_cmd::cmd_attach(JobKind::Watch, &name).await,
        WatchCommands::Logs {
            name,
            lines,
            follow,
        } => trigger_cmd::cmd_logs(JobKind::Watch, name.as_deref(), lines, follow).await,
        WatchCommands::Send { name, msg } => trigger_cmd::cmd_send(JobKind::Watch, &name, &msg),
        WatchCommands::Stop { name, force } => {
            trigger_cmd::cmd_stop(JobKind::Watch, &name, force).await
        }
        WatchCommands::Restart { name, dir } => {
            let dir = dir.as_deref().or(global_dir);
            trigger_cmd::cmd_restart(JobKind::Watch, &name, dir).await
        }
    }
}
