//! watch / cron 作业的起停与列表：CLI 与 daemon 共用同一份落盘规则。

use crate::definition::Directive;
use crate::supervisor::control::{ControlMsg, send_control};
use crate::supervisor::job::{JobKind, JobMeta};
use crate::supervisor::process::{
    child_supervisor_identity, kill_process_tree, spawn_detached,
};
use crate::trigger::{find_cron_trigger, find_watch_trigger};
use corex_core::EngineError;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 给宿主看的作业快照：不把内部 `JobMeta` 原样甩出去。
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub kind: JobKind,
    pub name: String,
    pub id: String,
    pub pid: u32,
    pub is_alive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    pub directive_path: PathBuf,
}

impl JobView {
    pub fn from_meta(meta: JobMeta) -> Self {
        let is_alive = meta.is_supervisor_alive();
        Self {
            kind: meta.kind,
            name: meta.directive_name,
            id: meta.id,
            pid: meta.pid,
            is_alive,
            started_at_ms: meta.started_at_ms,
            directive_path: meta.directive_path,
        }
    }
}

/// 指令必须声明对应族的触发器，否则起守护没有东西可跟。
pub fn ensure_trigger(kind: JobKind, directive: &Directive) -> Result<(), EngineError> {
    match kind {
        JobKind::Watch => {
            if find_watch_trigger(&directive.triggers)?.is_none() {
                return Err(EngineError::Usage(format!(
                    "指令 `{}` 未声明 watch 触发器",
                    directive.name
                )));
            }
        }
        JobKind::Cron => {
            if find_cron_trigger(&directive.triggers)?.is_none() {
                return Err(EngineError::Usage(format!(
                    "指令 `{}` 未声明 cron 触发器",
                    directive.name
                )));
            }
        }
    }
    Ok(())
}

/// 已登记的作业；`kind` 缺省则 cron 与 watch 都要。
pub fn jobs(data: &Path, kind: Option<JobKind>) -> Vec<JobView> {
    let kinds = match kind {
        Some(kind) => vec![kind],
        None => vec![JobKind::Cron, JobKind::Watch],
    };
    let mut out = Vec::new();
    for kind in kinds {
        JobMeta::prune_stale(data, kind);
        out.extend(JobMeta::scan(data, kind).into_iter().map(JobView::from_meta));
    }
    out
}

/// 拉起脱离终端的 supervisor 子进程，并写下 `meta.json`。
pub fn start_detached(
    data: &Path,
    kind: JobKind,
    directive_name: &str,
    directive_path: PathBuf,
    exe: &Path,
    args: &[String],
) -> Result<JobMeta, EngineError> {
    if let Some(existing) = JobMeta::find_running_by_directive(data, kind, directive_name) {
        return Err(EngineError::Conflict(format!(
            "指令 `{directive_name}` 已有 {} 守护运行中 (pid {})",
            kind.as_str(),
            existing.pid
        )));
    }
    let id = directive_name.to_string();
    let job_dir = JobMeta::job_dir(data, kind, &id);
    std::fs::create_dir_all(&job_dir)?;
    let log_path = JobMeta::supervisor_log_path(data, kind, &id);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let pid = spawn_detached(exe, &arg_refs, Some(&log_path))?;
    let (supervisor_exe, started_at_ms) = child_supervisor_identity(pid, exe);
    let meta = JobMeta {
        id,
        kind,
        directive_name: directive_name.to_string(),
        directive_path,
        pid,
        expr: None,
        paths: Vec::new(),
        supervisor_exe: Some(supervisor_exe),
        started_at_ms: Some(started_at_ms),
    };
    meta.write(data)?;
    Ok(meta)
}

fn find_job(data: &Path, kind: JobKind, name: &str) -> Result<JobMeta, EngineError> {
    JobMeta::resolve_by_name(data, kind, name).map_err(EngineError::DirectiveNotFound)
}

/// 优雅停止或强制杀掉 supervisor。
pub async fn stop_job(
    data: &Path,
    kind: JobKind,
    name: &str,
    force: bool,
) -> Result<JobView, EngineError> {
    let meta = find_job(data, kind, name)?;
    let job_dir = JobMeta::job_dir(data, kind, &meta.id);
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
                kill_process_tree(meta.pid).map_err(|err| {
                    EngineError::other(format!(
                        "强制终止 supervisor 进程树失败 (pid {}): {err}",
                        meta.pid
                    ))
                })?;
            }
        }
        let _ = JobMeta::remove(data, kind, &meta.id);
    } else {
        send_control(&job_dir, ControlMsg::Stop)?;
    }
    Ok(JobView::from_meta(meta))
}

/// 给运行中的 supervisor 写一条控制消息。
pub fn send_job(
    data: &Path,
    kind: JobKind,
    name: &str,
    command: &str,
) -> Result<JobView, EngineError> {
    let meta = find_job(data, kind, name)?;
    let job_dir = JobMeta::job_dir(data, kind, &meta.id);
    let control = command
        .parse::<ControlMsg>()
        .map_err(EngineError::Usage)?;
    send_control(&job_dir, control)?;
    Ok(JobView::from_meta(meta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_prunes_meta_without_identity() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let meta = JobMeta {
            id: "demo".into(),
            kind: JobKind::Cron,
            directive_name: "demo".into(),
            directive_path: PathBuf::from("corex.db"),
            pid: 999_999,
            expr: None,
            paths: vec![],
            supervisor_exe: None,
            started_at_ms: None,
        };
        meta.write(data).unwrap();
        let views = jobs(data, Some(JobKind::Cron));
        assert!(views.is_empty(), "没有身份字段的记录应被 prune");
    }
}
