//! 启动作业：要么拉起一个脱离终端的 supervisor，要么在本进程里充当 supervisor
//! （`--supervised` / `--foreground`）。

use crate::output::{errln, outln};
use anyhow::{Context, Result, bail};
use corex_core::EngineError;
use corex_engine::{
    ControlMsg, Directive, JobKind, JobMeta, child_supervisor_identity,
    current_supervisor_identity, send_control, spawn_detached, supervise_cron_job,
    supervise_watch_job,
};
use corex_ipc::data_dir;
use std::path::Path;

use super::{Jobs, Paths};

pub(crate) async fn start_job(
    kind: JobKind,
    target: &str,
    dir: Option<&Path>,
    immediate: bool,
) -> Result<()> {
    let path = Paths::resolve(target, dir)?;
    let directive = Directive::from_yaml_file(&path).context("解析指令")?;
    Jobs::ensure(kind, &directive)?;
    let data = data_dir()?;
    if let Some(existing) = Jobs::running(&data, kind, &directive.name) {
        bail!(
            "指令 `{}` 已有 {} 守护运行中 (pid {})。查看: corex {} attach {}",
            directive.name,
            Jobs::sub(kind),
            existing.pid,
            Jobs::sub(kind),
            directive.name
        );
    }
    let id = directive.name.clone();
    let sub = Jobs::sub(kind);
    let job_dir = JobMeta::job_dir(&data, kind, &id);
    std::fs::create_dir_all(&job_dir)?;
    let log_path = JobMeta::supervisor_log_path(&data, kind, &id);
    let exe = std::env::current_exe()?;
    let dir_arg = Paths::dir(dir)?.to_string_lossy().to_string();
    let mut args = vec![
        sub.to_string(),
        "run".to_string(),
        id.clone(),
        "--supervised".to_string(),
        "--job-id".to_string(),
        id.clone(),
        "--dir".to_string(),
        dir_arg,
    ];
    if immediate && kind == JobKind::Watch {
        args.push("--immediate".to_string());
    }
    let pid = spawn_detached(
        &exe,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        Some(&log_path),
    )?;
    let (supervisor_exe, started_at_ms) = child_supervisor_identity(pid, &exe);
    let meta = JobMeta {
        id: id.clone(),
        kind,
        directive_name: directive.name.clone(),
        directive_path: path,
        pid,
        expr: None,
        paths: Vec::new(),
        supervisor_exe: Some(supervisor_exe),
        started_at_ms: Some(started_at_ms),
    };
    meta.write(&data)?;
    outln!("已启动 {sub} `{id}` (pid {pid})");
    outln!("查看: corex {sub} attach {id}");
    Ok(())
}

/// 一次 `watch run` / `cron run` 该启动什么、怎么启动。
///
/// 收进一个结构体，两个调用方不会各自漂移，也不需要在调用点解读位置参数
/// `false` 是什么意思。
pub(crate) struct Spec<'a> {
    /// 作业所属的调度家族。
    pub kind: JobKind,
    /// 指令名；为 `None` 且 `all` 为真时运行全部已注册指令。
    pub name: Option<String>,
    /// 启动全部已注册指令。
    pub all: bool,
    /// 指令搜索目录覆盖。
    pub dir: Option<&'a Path>,
    /// 在当前终端运行，不另起 supervisor。
    pub foreground: bool,
    /// 先立即触发一次，再跟随触发器。
    pub immediate: bool,
    /// 作为 `job_id` 的 supervisor 子进程运行，不另起进程。
    pub supervised: bool,
    /// `supervised` 模式使用的 job id。
    pub job_id: Option<String>,
}

pub(crate) async fn cmd_run(req: Spec<'_>) -> Result<()> {
    let Spec {
        kind,
        name,
        all,
        dir,
        foreground,
        immediate,
        supervised,
        job_id,
    } = req;
    if supervised {
        let id = job_id
            .as_deref()
            .or(name.as_deref())
            .context("supervised 模式需要 job id")?;
        return cmd_supervised(kind, id, dir, immediate).await;
    }
    if foreground {
        if all {
            return Err(EngineError::Usage("--foreground 不能与 --all 同时使用".into()).into());
        }
        let n = name.context("--foreground 需要指定指令名")?;
        return cmd_foreground(kind, &n, dir, immediate).await;
    }
    if all {
        return start_all(kind, dir, immediate).await;
    }
    if let Some(n) = name {
        return start_job(kind, &n, dir, immediate).await;
    }
    Err(EngineError::Usage("需要指令名或 --all".into()).into())
}

async fn cmd_supervised(
    kind: JobKind,
    job_id: &str,
    dir: Option<&Path>,
    immediate: bool,
) -> Result<()> {
    let data = data_dir()?;
    let meta = JobMeta::read(&data, kind, job_id).context("读取 job meta")?;
    let _dir = dir;
    let store = Jobs::store();
    let runtime = crate::settings::effective().clone();
    match kind {
        JobKind::Watch => supervise_watch_job(&meta, store, runtime, &data, immediate)
            .await
            .map_err(anyhow::Error::new)?,
        JobKind::Cron => supervise_cron_job(&meta, store, runtime, &data)
            .await
            .map_err(anyhow::Error::new)?,
    }
    Ok(())
}

async fn cmd_foreground(
    kind: JobKind,
    target: &str,
    dir: Option<&Path>,
    immediate: bool,
) -> Result<()> {
    let path = Paths::resolve(target, dir)?;
    let directive = Directive::from_yaml_file(&path)?;
    Jobs::ensure(kind, &directive)?;
    let data = data_dir()?;
    if Jobs::running(&data, kind, &directive.name).is_some() {
        bail!(
            "指令 `{}` 已有 {} 守护运行中，请用 attach 查看",
            directive.name,
            Jobs::sub(kind)
        );
    }
    let id = directive.name.clone();
    let job_dir = JobMeta::job_dir(&data, kind, &id);
    std::fs::create_dir_all(&job_dir)?;
    let (supervisor_exe, started_at_ms) = current_supervisor_identity();
    let meta = JobMeta {
        id: id.clone(),
        kind,
        directive_name: directive.name.clone(),
        directive_path: path,
        pid: std::process::id(),
        expr: None,
        paths: Vec::new(),
        supervisor_exe: Some(supervisor_exe),
        started_at_ms: Some(started_at_ms),
    };
    meta.write(&data)?;
    let store = Jobs::store();
    let runtime = crate::settings::effective().clone();
    tokio::select! {
        res = async {
            match kind {
                JobKind::Watch => {
                    supervise_watch_job(&meta, store, runtime, &data, immediate).await
                }
                JobKind::Cron => supervise_cron_job(&meta, store, runtime, &data).await,
            }
        } => res.map_err(anyhow::Error::new)?,
        _ = tokio::signal::ctrl_c() => {
            let job_dir = JobMeta::job_dir(&data, kind, &id);
            if let Err(err) = send_control(&job_dir, ControlMsg::Stop) {
                errln!("提示: 发送 STOP 失败（{err}）");
            }
            outln!("已停止前台 {} `{id}`", Jobs::sub(kind));
        }
    }
    Ok(())
}

async fn start_all(kind: JobKind, dir: Option<&Path>, immediate: bool) -> Result<()> {
    let base = Paths::dir(dir)?;
    if !base.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&base)? {
        let entry = entry?;
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        ) {
            continue;
        }
        let directive = Directive::from_yaml_file(&path)?;
        let declares = match kind {
            JobKind::Watch => corex_engine::find_watch_trigger(&directive.triggers)?.is_some(),
            JobKind::Cron => corex_engine::find_cron_trigger(&directive.triggers)?.is_some(),
        };
        if declares {
            start_job(kind, &directive.name, dir, immediate).await?;
        }
    }
    Ok(())
}
