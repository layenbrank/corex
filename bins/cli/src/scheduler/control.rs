//! 查看与控制运行中的作业：列表、停止、发送控制消息、重启。

use crate::output::{errln, outln};
use anyhow::{Context, Result};
use corex_engine::{ControlMsg, JobKind, JobMeta, kill_process_tree, send_control};
use corex_ipc::data_dir;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::time::Duration;

use super::Jobs;
use super::start::start_job;

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

pub(crate) fn cmd_ps(kind: JobKind) -> Result<()> {
    let data = data_dir()?;
    let sub = Jobs::sub(kind);
    JobMeta::prune_stale(&data, kind);
    let jobs = JobMeta::scan(&data, kind);
    if jobs.is_empty() {
        outln!("(无 {sub} job)");
        return Ok(());
    }
    let color = io::stdout().is_terminal();
    outln!("{:<20} {:<10} {:<8} 指令", "名称", "状态", "PID");
    for j in jobs {
        let online = j.is_supervisor_alive();
        let status = if online { "运行中" } else { "已停止" };
        if color {
            let styled = if online {
                format!("{GREEN}{status}{RESET}")
            } else {
                format!("{RED}{status}{RESET}")
            };
            outln!(
                "{:<20} {:<19} {:<8} {}",
                j.directive_name,
                styled,
                j.pid,
                j.directive_path.display()
            );
        } else {
            outln!(
                "{:<20} {:<10} {:<8} {}",
                j.directive_name,
                status,
                j.pid,
                j.directive_path.display()
            );
        }
    }
    if color {
        errln!("提示: 操作请使用 NAME 列指令名，非 PID");
    }
    Ok(())
}

pub(crate) async fn cmd_stop(kind: JobKind, name: &str, force: bool) -> Result<()> {
    let data = data_dir()?;
    let meta = Jobs::find(kind, name)?;
    let job_dir = JobMeta::job_dir(&data, kind, &meta.id);
    if force {
        if meta.is_supervisor_alive() {
            send_control(&job_dir, ControlMsg::StopForce)?;
            for _ in 0..24 {
                if !meta.is_supervisor_alive() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            if meta.is_supervisor_alive() {
                kill_process_tree(meta.pid).with_context(|| {
                    format!("强制终止 supervisor 进程树失败 (pid {})", meta.pid)
                })?;
            }
        }
        if let Err(err) = JobMeta::remove(&data, kind, &meta.id) {
            errln!("提示: 删除 job 元数据失败（{err}）");
        }
        outln!("已强制停止 → {}", meta.directive_name);
    } else {
        send_control(&job_dir, ControlMsg::Stop)?;
        outln!(
            "已发送 STOP → {}（优雅停止，进行中的构建会跑完）",
            meta.directive_name
        );
    }
    Ok(())
}

pub(crate) fn cmd_send(kind: JobKind, name: &str, msg: &str) -> Result<()> {
    let data = data_dir()?;
    let meta = Jobs::find(kind, name)?;
    let job_dir = JobMeta::job_dir(&data, kind, &meta.id);
    let control = msg.parse::<ControlMsg>().map_err(|e| anyhow::anyhow!(e))?;
    send_control(&job_dir, control)?;
    outln!("已发送 {control} → {}", meta.directive_name);
    Ok(())
}

pub(crate) async fn cmd_restart(kind: JobKind, name: &str, dir: Option<&Path>) -> Result<()> {
    // 停止失败要明说：接下来的启动要么被拒（“已在运行”），要么留下两个 supervisor，
    // 用户得知道为什么。
    if let Err(err) = cmd_stop(kind, name, false).await {
        errln!(
            "提示: 停止现有 {} 失败（{err}），仍将尝试启动",
            Jobs::sub(kind)
        );
    }
    tokio::time::sleep(Duration::from_millis(800)).await;
    start_job(kind, name, dir, false).await
}
