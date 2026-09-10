//! 跟踪作业的 supervisor 日志，并 attach 到正在运行的作业。

use crate::output::{self, errln, outln};
use anyhow::Result;
use corex_engine::{JobKind, JobMeta};
use corex_ipc::data_dir;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::Jobs;

pub(crate) async fn cmd_attach(kind: JobKind, name: &str) -> Result<()> {
    let data = data_dir()?;
    let meta = Jobs::find(kind, name)?;
    let log_path = JobMeta::supervisor_log_path(&data, kind, &meta.id);
    let status = if meta.is_supervisor_alive() {
        "运行中"
    } else {
        "已停止"
    };
    outln!(
        "=== {} {} | 状态={} pid={} ===",
        Jobs::sub(kind),
        meta.directive_name,
        status,
        meta.pid
    );
    outln!("日志: {}", log_path.display());
    if !log_path.exists() {
        outln!("(尚无日志，等待 supervisor 输出…)");
    }
    tail(&log_path, true, 50).await?;
    if meta.is_supervisor_alive() {
        outln!(
            "已退出查看，`{}` 仍在运行 (pid {})",
            meta.directive_name,
            meta.pid
        );
    } else {
        outln!("已退出查看，`{}` 已停止", meta.directive_name);
    }
    Ok(())
}

pub(crate) async fn cmd_logs(
    kind: JobKind,
    name: Option<&str>,
    lines: usize,
    follow: bool,
) -> Result<()> {
    let data = data_dir()?;
    if let Some(n) = name {
        let meta = Jobs::find(kind, n)?;
        let log_path = JobMeta::supervisor_log_path(&data, kind, &meta.id);
        tail(&log_path, follow, lines).await?;
        Ok(())
    } else {
        let jobs = JobMeta::scan(&data, kind);
        if jobs.is_empty() {
            outln!("(无 {} job)", Jobs::sub(kind));
            return Ok(());
        }
        for j in jobs {
            let log = JobMeta::supervisor_log_path(&data, kind, &j.id);
            outln!("{}  {}", j.directive_name, log.display());
        }
        Ok(())
    }
}

async fn tail(path: &Path, follow: bool, lines: usize) -> Result<()> {
    if path.exists() {
        preview(path, lines)?;
    } else if follow {
        std::fs::File::create(path)?;
    }
    if !follow {
        return Ok(());
    }
    let mut file = tokio::fs::OpenOptions::new().read(true).open(path).await?;
    file.seek(std::io::SeekFrom::End(0)).await?;
    let mut buf = vec![0u8; 4096];
    loop {
        tokio::select! {
            res = tokio::signal::ctrl_c() => {
                res?;
                break;
            }
            n = file.read(&mut buf) => {
                let n = n?;
                if n == 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    continue;
                }
                // 原始字节流，不能走 `outln!`；但关闭管道的策略仍归输出层管。
                output::bytes(&buf[..n])?;
                // `bytes` 会故意吞掉 EPIPE。没有这一步，跟踪会不停地轮询一个没人读的管道
                // （`attach | head` 永远不返回）。
                if output::is_stdout_closed() {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn preview(path: &Path, lines: usize) -> Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            // 文件不存在是首次运行的正常情况（supervisor 紧接着会创建它）；其他错误只会让
            // 用户对着空视图发呆，所以要说清原因。
            if err.kind() != std::io::ErrorKind::NotFound {
                errln!("提示: 无法读取日志 {}（{err}）", path.display());
            }
            String::new()
        }
    };
    if text.is_empty() {
        return Ok(());
    }
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    for line in &all[start..] {
        outln!("{line}");
    }
    Ok(())
}
