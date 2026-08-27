//! Shared helpers for watch/cron CLI commands.

use anyhow::{bail, Context, Result};
use corex_core::RuntimeConfig;
use corex_engine::{
    supervise_cron_job, supervise_watch_job, is_pid_running, send_control, spawn_detached, ControlMsg,
    JobKind, JobMeta, Directive,
};
use corex_ipc::platform_data_dir;
use corex_registry::ActionRegistry;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

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

fn kind_sub(kind: JobKind) -> &'static str {
    match kind {
        JobKind::Watch => "watch",
        JobKind::Cron => "cron",
    }
}

fn resolve_job(kind: JobKind, name: &str) -> Result<JobMeta> {
    let data = platform_data_dir()?;
    JobMeta::resolve_by_name(&data, kind, name).map_err(|e| anyhow::anyhow!(e))
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

fn find_running_job(data: &Path, kind: JobKind, directive_name: &str) -> Option<JobMeta> {
    JobMeta::find_running_by_directive(data, kind, directive_name, is_pid_running)
}

pub async fn start_job(kind: JobKind, target: &str, dir: Option<&Path>) -> Result<()> {
    let path = resolve_directive_path(target, dir)?;
    let directive = Directive::from_yaml_file(&path).context("解析指令")?;
    ensure_trigger_declared(kind, &directive)?;
    let data = platform_data_dir()?;
    if let Some(existing) = find_running_job(&data, kind, &directive.name) {
        bail!(
            "指令 `{}` 已有 {} 守护运行中 (pid {})。查看: corex {} attach {}",
            directive.name,
            kind_sub(kind),
            existing.pid,
            kind_sub(kind),
            directive.name
        );
    }
    let id = directive.name.clone();
    let sub = kind_sub(kind);
    let job_dir = JobMeta::job_dir(&data, kind, &id);
    std::fs::create_dir_all(&job_dir)?;
    let log_path = JobMeta::supervisor_log_path(&data, kind, &id);
    let exe = std::env::current_exe()?;
    let dir_arg = directives_dir(dir)?.to_string_lossy().to_string();
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
            &dir_arg,
        ],
        Some(&log_path),
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
    println!("查看: corex {sub} attach {id}");
    Ok(())
}

pub fn cmd_ps(kind: JobKind) -> Result<()> {
    let data = platform_data_dir()?;
    let sub = kind_sub(kind);
    let jobs = JobMeta::scan(&data, kind);
    if jobs.is_empty() {
        println!("(无 {sub} job)");
        return Ok(());
    }
    let color = io::stdout().is_terminal();
    println!("{:<20} {:<10} {:<8} {}", "NAME", "STATUS", "PID", "DIRECTIVE");
    for j in jobs {
        let online = is_pid_running(j.pid);
        let status = if online { "online" } else { "stopped" };
        if color {
            let styled = if online {
                format!("{GREEN}{status}{RESET}")
            } else {
                format!("{RED}{status}{RESET}")
            };
            println!(
                "{:<20} {:<19} {:<8} {}",
                j.directive_name,
                styled,
                j.pid,
                j.directive_path.display()
            );
        } else {
            println!(
                "{:<20} {:<10} {:<8} {}",
                j.directive_name,
                status,
                j.pid,
                j.directive_path.display()
            );
        }
    }
    if color {
        eprintln!("提示: 操作请使用 NAME 列指令名，非 PID");
    }
    Ok(())
}

pub fn cmd_stop(kind: JobKind, name: &str) -> Result<()> {
    let data = platform_data_dir()?;
    let meta = resolve_job(kind, name)?;
    let job_dir = JobMeta::job_dir(&data, kind, &meta.id);
    send_control(&job_dir, ControlMsg::Stop)?;
    println!("已发送 STOP → {}", meta.directive_name);
    Ok(())
}

pub fn cmd_send(kind: JobKind, name: &str, msg: &str) -> Result<()> {
    let data = platform_data_dir()?;
    let meta = resolve_job(kind, name)?;
    let job_dir = JobMeta::job_dir(&data, kind, &meta.id);
    let control = msg.parse::<ControlMsg>().map_err(|e| anyhow::anyhow!(e))?;
    send_control(&job_dir, control)?;
    println!("已发送 {control} → {}", meta.directive_name);
    Ok(())
}

pub async fn cmd_attach(kind: JobKind, name: &str) -> Result<()> {
    let data = platform_data_dir()?;
    let meta = resolve_job(kind, name)?;
    let log_path = JobMeta::supervisor_log_path(&data, kind, &meta.id);
    let status = if is_pid_running(meta.pid) {
        "online"
    } else {
        "stopped"
    };
    println!(
        "=== {} {} | status={} pid={} ===",
        kind_sub(kind),
        meta.directive_name,
        status,
        meta.pid
    );
    println!("日志: {}", log_path.display());
    if !log_path.exists() {
        println!("(尚无日志，等待 supervisor 输出…)");
    }
    tail_log(&log_path, true, 50).await?;
    if is_pid_running(meta.pid) {
        println!(
            "已退出查看，`{}` 仍在运行 (pid {})",
            meta.directive_name, meta.pid
        );
    } else {
        println!("已退出查看，`{}` 已停止", meta.directive_name);
    }
    Ok(())
}

pub async fn cmd_logs(kind: JobKind, name: Option<&str>, lines: usize, follow: bool) -> Result<()> {
    let data = platform_data_dir()?;
    if let Some(n) = name {
        let meta = resolve_job(kind, n)?;
        let log_path = JobMeta::supervisor_log_path(&data, kind, &meta.id);
        tail_log(&log_path, follow, lines).await?;
        Ok(())
    } else {
        let jobs = JobMeta::scan(&data, kind);
        if jobs.is_empty() {
            println!("(无 {} job)", kind_sub(kind));
            return Ok(());
        }
        for j in jobs {
            let log = JobMeta::supervisor_log_path(&data, kind, &j.id);
            println!("{}  {}", j.directive_name, log.display());
        }
        Ok(())
    }
}

async fn tail_log(path: &Path, follow: bool, lines: usize) -> Result<()> {
    if path.exists() {
        print_last_lines(path, lines)?;
    } else if follow {
        std::fs::File::create(path)?;
    }
    if !follow {
        return Ok(());
    }
    let mut file = tokio::fs::OpenOptions::new()
        .read(true)
        .open(path)
        .await?;
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
                io::stdout().write_all(&buf[..n])?;
                io::stdout().flush()?;
            }
        }
    }
    Ok(())
}

fn print_last_lines(path: &Path, lines: usize) -> Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    if text.is_empty() {
        return Ok(());
    }
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    for line in &all[start..] {
        println!("{line}");
    }
    Ok(())
}

pub async fn cmd_run_interactive(
    kind: JobKind,
    name: &str,
    dir: Option<&Path>,
    foreground: bool,
) -> Result<()> {
    let data = platform_data_dir()?;
    if !foreground {
        if find_running_job(&data, kind, name).is_some() {
            return cmd_attach(kind, name).await;
        }
        bail!(
            "请先 corex {} start {name}，或使用 --foreground 前台开发模式",
            kind_sub(kind)
        );
    }
    cmd_run_foreground(kind, name, dir).await
}

pub async fn cmd_run_supervised(
    kind: JobKind,
    job_id: &str,
    dir: Option<&Path>,
) -> Result<()> {
    let data = platform_data_dir()?;
    let meta = JobMeta::read(&data, kind, job_id).context("读取 job meta")?;
    let _dir = dir;
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
    if find_running_job(&data, kind, &directive.name).is_some() {
        bail!(
            "指令 `{}` 已有 {} 守护运行中，请用 attach 查看",
            directive.name,
            kind_sub(kind)
        );
    }
    let id = directive.name.clone();
    let job_dir = JobMeta::job_dir(&data, kind, &id);
    std::fs::create_dir_all(&job_dir)?;
    let meta = JobMeta {
        id: id.clone(),
        kind,
        directive_name: directive.name.clone(),
        directive_path: path,
        pid: std::process::id(),
        expr: None,
        paths: Vec::new(),
    };
    meta.write(&data)?;
    let store = build_store();
    let runtime = load_runtime_config();
    tokio::select! {
        res = async {
            match kind {
                JobKind::Watch => supervise_watch_job(&meta, store, runtime, &data).await,
                JobKind::Cron => supervise_cron_job(&meta, store, runtime, &data).await,
            }
        } => res.map_err(|e| anyhow::anyhow!("{e}"))?,
        _ = tokio::signal::ctrl_c() => {
            let job_dir = JobMeta::job_dir(&data, kind, &id);
            let _ = send_control(&job_dir, ControlMsg::Stop);
            println!("已停止前台 {} `{id}`", kind_sub(kind));
        }
    }
    Ok(())
}

pub async fn cmd_restart(kind: JobKind, name: &str, dir: Option<&Path>) -> Result<()> {
    let _ = cmd_stop(kind, name);
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    start_job(kind, name, dir).await
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
