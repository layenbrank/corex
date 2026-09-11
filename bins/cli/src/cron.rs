//! `corex cron` 子命令。

use crate::scheduler;
use anyhow::Result;
use clap::Subcommand;
use corex_engine::JobKind;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum CronCommands {
    /// 运行 cron supervisor（默认后台；开发时用 --foreground）
    Run {
        /// 指令名（配 --all 时可省略）
        name: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        dir: Option<PathBuf>,
        /// 前台开发模式，占用当前终端（Ctrl+C 停止 supervisor）
        #[arg(long)]
        foreground: bool,
        #[arg(long, hide = true)]
        supervised: bool,
        #[arg(long, hide = true)]
        job_id: Option<String>,
    },
    /// 列出运行中的 cron 作业（后续命令用 NAME 列的值）
    Ps,
    /// 实时跟踪 supervisor 日志（Ctrl+C 仅断开，不停止 supervisor）
    Attach {
        /// 指令名
        name: String,
    },
    /// 跟踪 supervisor 日志
    Logs {
        name: Option<String>,
        #[arg(long, default_value_t = 50)]
        lines: usize,
        #[arg(short, long)]
        follow: bool,
    },
    /// 发送控制消息：run-now | status | stop
    Send { name: String, msg: String },
    /// 停止 cron 作业（--force 会终止进行中的构建）
    Stop {
        /// 指令名
        name: String,
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// 重启 cron 作业
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
            scheduler::run(scheduler::Spec {
                kind: JobKind::Cron,
                name,
                all,
                dir,
                is_foreground: foreground,
                // `cron run` 没有 `--immediate`；cron 触发器本身就是时间驱动的。
                immediate: false,
                is_supervised: supervised,
                job_id,
            })
            .await
        }
        CronCommands::Ps => scheduler::ps(JobKind::Cron),
        CronCommands::Attach { name } => scheduler::attach(JobKind::Cron, &name).await,
        CronCommands::Logs {
            name,
            lines,
            follow,
        } => scheduler::logs(JobKind::Cron, name.as_deref(), lines, follow).await,
        CronCommands::Send { name, msg } => scheduler::send(JobKind::Cron, &name, &msg),
        CronCommands::Stop { name, force } => scheduler::stop(JobKind::Cron, &name, force).await,
        CronCommands::Restart { name, dir } => {
            let dir = dir.as_deref().or(global_dir);
            scheduler::restart(JobKind::Cron, &name, dir).await
        }
    }
}
