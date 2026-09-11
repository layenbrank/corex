//! 查看与控制运行中的作业：列表、停止、发送控制消息、重启。

use crate::output::{Role, errln, outln, paint};
use anyhow::{Context, Result};
use corex_engine::{ControlMsg, JobKind, JobMeta, kill_process_tree, send_control};
use corex_ipc::data_dir;
use std::path::Path;
use std::time::Duration;

use super::Jobs;
use super::start::start_job;

pub(crate) fn ps(kind: JobKind) -> Result<()> {
    let data = data_dir()?;
    let sub = Jobs::sub(kind);
    JobMeta::prune_stale(&data, kind);
    let jobs = JobMeta::scan(&data, kind);
    if jobs.is_empty() {
        outln!("(无 {sub} job)");
        return Ok(());
    }
    outln!(
        "{:<20} {:<10} {:<8} {:<8} 指令",
        "名称",
        "状态",
        "PID",
        "已运行"
    );
    for j in jobs {
        let online = j.is_supervisor_alive();
        let (status, role) = if online {
            ("运行中", Role::Ok)
        } else {
            ("已停止", Role::Bad)
        };
        // 先按纯文本对齐再上色：转义序列不参与格式宽度，否则列会歪。
        let status = paint(role, &format!("{status:<10}"));
        // 已停止的 job 没有「运行了多久」可言，留空比编一个数字诚实。
        let uptime = if online {
            uptime(j.started_at_ms)
        } else {
            String::new()
        };
        outln!(
            "{:<20} {} {:<8} {:<8} {}",
            j.directive_name,
            status,
            j.pid,
            uptime,
            j.directive_path.display()
        );
    }
    // 提示与颜色无关：重定向进文件或 CI 日志时同样需要它。
    errln!("提示: 操作请使用 NAME 列指令名，非 PID");
    Ok(())
}

/// 已经跑了多久：`2h13m` / `13m05s` / `45s`。
///
/// 看得出「这东西到底有没有在干活」常常就靠这一列。
fn uptime(started_at_ms: Option<u64>) -> String {
    let Some(started) = started_at_ms else {
        return String::new();
    };
    let secs = (chrono::Utc::now().timestamp_millis() - started as i64).max(0) / 1000;
    let (hours, minutes) = (secs / 3600, (secs % 3600) / 60);
    if hours > 0 {
        format!("{hours}h{minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m{:02}s", secs % 60)
    } else {
        format!("{}s", secs % 60)
    }
}

pub(crate) async fn stop(kind: JobKind, name: &str, force: bool) -> Result<()> {
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

pub(crate) fn send(kind: JobKind, name: &str, msg: &str) -> Result<()> {
    let data = data_dir()?;
    let meta = Jobs::find(kind, name)?;
    let job_dir = JobMeta::job_dir(&data, kind, &meta.id);
    let control = msg.parse::<ControlMsg>().map_err(|e| anyhow::anyhow!(e))?;
    send_control(&job_dir, control)?;
    outln!("已发送 {control} → {}", meta.directive_name);
    Ok(())
}

pub(crate) async fn restart(kind: JobKind, name: &str, dir: Option<&Path>) -> Result<()> {
    // 停止失败要明说：接下来的启动要么被拒（“已在运行”），要么留下两个 supervisor，
    // 用户得知道为什么。
    if let Err(err) = stop(kind, name, false).await {
        errln!(
            "提示: 停止现有 {} 失败（{err}），仍将尝试启动",
            Jobs::sub(kind)
        );
    }
    tokio::time::sleep(Duration::from_millis(800)).await;
    start_job(kind, name, dir, false).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_is_compact() {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        assert_eq!(uptime(Some(now - 30_000)), "30s");
        assert_eq!(uptime(Some(now - 13 * 60_000)), "13m00s");
        assert_eq!(uptime(Some(now - 2 * 3_600_000)), "2h00m");
        // 没有起点（老 job 或换了机器）就不编时间。
        assert_eq!(uptime(None), "");
    }
}
