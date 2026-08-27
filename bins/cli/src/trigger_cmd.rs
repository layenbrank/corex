//! Shared helpers for watch/cron CLI commands.

use anyhow::{bail, Context, Result};
use corex_core::RuntimeConfig;
use corex_engine::{
    supervise_cron_job, supervise_watch_job, is_pid_running, send_control, spawn_detached, ControlMsg,
    JobKind, JobMeta, Directive,
};
use corex_ipc::platform_data_dir;
use corex_registry::ActionRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn resolve_directive_path(target: &str, dir: Option<&Path>) -> Result<PathBuf> {
    let as_path = PathBuf::from(target);
    if as_path.exists() {
        return Ok(as_path);
    }
    let base = directives_dir(dir)?;
    for ext in ["yaml", "yml"] {
        let p = base.join(format!("{target}.{ext}"));
        if p.exists() {
            return Ok(p);
        }
    }
    let examples = PathBuf::from("examples/directives");
    for ext in ["yaml", "yml"] {
        let p = examples.join(format!("{target}.{ext}"));
        if p.exists() {
            return Ok(p);
        }
    }
    bail!("指令未找到: {target}");
}

pub fn directives_dir(override_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(d) = override_dir {
        return Ok(d.to_path_buf());
    }
    let d = platform_data_dir()?.join("directives");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn build_store() -> Arc<ActionRegistry> {
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    reg.apply_runtime_config(&load_runtime_config());
    Arc::new(reg)
}

pub fn load_runtime_config() -> RuntimeConfig {
    crate::load_runtime_config()
}

fn ensure_trigger_declared(kind: JobKind, directive: &Directive) -> Result<()> {
    match kind {
        JobKind::Watch => {
            if corex_engine::find_watch_trigger(&directive.triggers)?.is_none() {
                bail!("指令 `{}` 未声明 watch 触发器", directive.name);
            }
        }
        JobKind::Cron => {
            if corex_engine::find_cron_trigger(&directive.triggers)?.is_none() {
                bail!("指令 `{}` 未声明 cron 触发器", directive.name);
            }
        }
    }
    Ok(())
}

fn ensure_directive_not_running(data: &Path, kind: JobKind, directive_name: &str) -> Result<()> {
    let sub = match kind {
        JobKind::Watch => "watch",
        JobKind::Cron => "cron",
    };
    if let Some(existing) =
        JobMeta::find_running_by_directive(data, kind, directive_name, is_pid_running)
    {
        bail!(
            "指令 `{directive_name}` 已有 {sub} 守护运行中 (job `{}`, pid {})",
            existing.id,
            existing.pid
        );
    }
    Ok(())
}

pub async fn start_job(kind: JobKind, target: &str, dir: Option<&Path>) -> Result<()> {
    let path = resolve_directive_path(target, dir)?;
    let directive = Directive::from_yaml_file(&path).context("解析指令")?;
    ensure_trigger_declared(kind, &directive)?;
    let data = platform_data_dir()?;
    ensure_directive_not_running(&data, kind, &directive.name)?;
    let id = directive.name.clone();
    let sub = match kind {
        JobKind::Watch => "watch",
        JobKind::Cron => "cron",
    };
    let exe = std::env::current_exe()?;
    let pid = spawn_detached(
        &exe,
        &[
            sub,
            "run",
            &id,
            "--supervised",
            "--job-id",
            &id,
            "--dir",
            &directives_dir(dir)?.to_string_lossy(),
        ],
    )?;
    let meta = JobMeta {
        id: id.clone(),
        kind,
        directive_name: directive.name.clone(),
        directive_path: path,
        pid,
        expr: None,
        paths: Vec::new(),
    };
    meta.write(&data)?;
    println!("已启动 {sub} `{id}` (pid {pid})");
    Ok(())
}

pub fn cmd_ps(kind: JobKind) -> Result<()> {
    let data = platform_data_dir()?;
    let sub = match kind {
        JobKind::Watch => "watch",
        JobKind::Cron => "cron",
    };
    let jobs = JobMeta::scan(&data, kind);
    if jobs.is_empty() {
        println!("(无 {sub} job)");
        return Ok(());
    }
    for j in jobs {
        let status = if is_pid_running(j.pid) {
            "online"
        } else {
            "stopped"
        };
        println!(
            "{:<20} {:<8} pid={:<8} {}",
            j.id, status, j.pid, j.directive_path.display()
        );
    }
    Ok(())
}

pub fn cmd_stop(kind: JobKind, id: &str) -> Result<()> {
    let data = platform_data_dir()?;
    let job_dir = JobMeta::job_dir(&data, kind, id);
    send_control(&job_dir, ControlMsg::Stop)?;
    println!("已发送 STOP → {id}");
    Ok(())
}

pub fn cmd_send(kind: JobKind, id: &str, msg: &str) -> Result<()> {
    let data = platform_data_dir()?;
    let job_dir = JobMeta::job_dir(&data, kind, id);
    let control = msg.parse::<ControlMsg>().map_err(|e| anyhow::anyhow!(e))?;
    send_control(&job_dir, control)?;
    println!("已发送 {control} → {id}");
    Ok(())
}

pub async fn cmd_run_supervised(
    kind: JobKind,
    job_id: &str,
    dir: Option<&Path>,
) -> Result<()> {
    let data = platform_data_dir()?;
    let meta = JobMeta::read(&data, kind, job_id).context("读取 job meta")?;
    let store = build_store();
    let runtime = load_runtime_config();
    match kind {
        JobKind::Watch => supervise_watch_job(&meta, store, runtime, &data)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?,
        JobKind::Cron => supervise_cron_job(&meta, store, runtime, &data)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    }
    Ok(())
}

pub async fn cmd_run_foreground(kind: JobKind, target: &str, dir: Option<&Path>) -> Result<()> {
    let path = resolve_directive_path(target, dir)?;
    let directive = Directive::from_yaml_file(&path)?;
    ensure_trigger_declared(kind, &directive)?;
    let data = platform_data_dir()?;
    ensure_directive_not_running(&data, kind, &directive.name)?;
    let meta = JobMeta {
        id: directive.name.clone(),
        kind,
        directive_name: directive.name.clone(),
        directive_path: path,
        pid: std::process::id(),
        expr: None,
        paths: Vec::new(),
    };
    let store = build_store();
    let runtime = load_runtime_config();
    match kind {
        JobKind::Watch => supervise_watch_job(&meta, store, runtime, &data)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?,
        JobKind::Cron => supervise_cron_job(&meta, store, runtime, &data)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    }
    Ok(())
}

pub async fn start_all(kind: JobKind, dir: Option<&Path>) -> Result<()> {
    let base = directives_dir(dir)?;
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
        let has = match kind {
            JobKind::Watch => corex_engine::find_watch_trigger(&directive.triggers)?.is_some(),
            JobKind::Cron => corex_engine::find_cron_trigger(&directive.triggers)?.is_some(),
        };
        if has {
            start_job(kind, &directive.name, dir).await?;
        }
    }
    Ok(())
}
