//! `corex watch` 子命令。

use crate::scheduler;
use anyhow::Result;
use clap::Subcommand;
use corex_engine::JobKind;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum WatchCommands {
    /// 运行 watch supervisor（默认后台；开发时用 --foreground）
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
        /// watch 启动后立即运行一次流水线
        #[arg(long)]
        immediate: bool,
        #[arg(long, hide = true)]
        supervised: bool,
        #[arg(long, hide = true)]
        job_id: Option<String>,
    },
    /// 列出运行中的 watch 作业（后续命令用 NAME 列的值）
    Ps,
    /// 实时跟踪 supervisor 日志（Ctrl+C 仅断开，不停止 supervisor）
    Attach {
        /// 指令名（如 build-client）
        name: String,
    },
    /// 跟踪 supervisor 日志
    Logs {
        /// 指令名；省略则列出日志路径
        name: Option<String>,
        #[arg(long, default_value_t = 50)]
        lines: usize,
        #[arg(short, long)]
        follow: bool,
    },
    /// 发送控制消息：run-now | status | stop
    Send {
        /// 指令名
        name: String,
        msg: String,
    },
    /// 停止 watch 作业（--force 会终止进行中的构建）
    Stop {
        /// 指令名
        name: String,
        /// 立即终止 supervisor 与进行中的流水线
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// 重启 watch 作业
    Restart {
        /// 指令名
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
            scheduler::cmd_run(scheduler::Spec {
                kind: JobKind::Watch,
                name,
                all,
                dir,
                foreground,
                immediate,
                supervised,
                job_id,
            })
            .await
        }
        WatchCommands::Ps => scheduler::cmd_ps(JobKind::Watch),
        WatchCommands::Attach { name } => scheduler::cmd_attach(JobKind::Watch, &name).await,
        WatchCommands::Logs {
            name,
            lines,
            follow,
        } => scheduler::cmd_logs(JobKind::Watch, name.as_deref(), lines, follow).await,
        WatchCommands::Send { name, msg } => scheduler::cmd_send(JobKind::Watch, &name, &msg),
        WatchCommands::Stop { name, force } => {
            scheduler::cmd_stop(JobKind::Watch, &name, force).await
        }
        WatchCommands::Restart { name, dir } => {
            let dir = dir.as_deref().or(global_dir);
            scheduler::cmd_restart(JobKind::Watch, &name, dir).await
        }
    }
}
