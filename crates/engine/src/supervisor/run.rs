//! watch 与 cron 的 supervisor 运行循环。

#[cfg(feature = "watch")]
pub async fn supervise_watch_job(
    meta: &crate::supervisor::JobMeta,
    store: std::sync::Arc<dyn corex_core::ActionStore>,
    runtime: corex_core::RuntimeConfig,
    data_dir: &std::path::Path,
    immediate_cli: bool,
    io: &crate::run::SupervisorIo,
) -> Result<(), corex_core::EngineError> {
    use crate::definition::Directive;
    use crate::supervisor::process::kill_process_tree;
    use crate::supervisor::resolve::resolve_watch_config;
    use crate::supervisor::{ControlMsg, JobKind, JobMeta, poll_control};
    use crate::trigger::find_watch_trigger;
    use crate::watch::{WatchEngine, WatchJobSpec};
    use std::time::Duration;
    use tracing::info;

    /// 优雅停止等待当前轮的上限；超时改走强制停，避免 `is_running` 卡死挂住 supervisor
    const STOP_IDLE_TIMEOUT: Duration = Duration::from_secs(3_600);

    // 指令优先从库里取：v13 起真相在库，磁盘上的那份可能还是迁移前留下的旧版本。
    let directive = match &io.source {
        Some(source) => source.load(&meta.directive_name)?,
        None => Directive::from_yaml_file(&meta.directive_path)?,
    };
    let engine = WatchEngine::new(data_dir.to_path_buf(), store, runtime.clone(), io.clone());
    let watch_raw = find_watch_trigger(&directive.triggers)?.ok_or_else(|| {
        corex_core::EngineError::other(format!("指令 {} 未声明 watch 触发器", meta.directive_name))
    })?;
    let mut watch = resolve_watch_config(&directive, runtime, watch_raw)?;
    if immediate_cli {
        watch.immediate = true;
    }
    engine
        .register(WatchJobSpec {
            id: meta.id.clone(),
            directive_path: meta.directive_path.clone(),
            directive_name: meta.directive_name.clone(),
            config: watch.clone(),
        })
        .await?;
    if watch.immediate
        && let Err(e) = engine.run_now(&meta.id).await
    {
        tracing::warn!(job = %meta.id, error = %e, "watch immediate run_now 失败");
    }
    let job_dir = JobMeta::job_dir(data_dir, JobKind::Watch, &meta.id);
    info!(job = %meta.id, "watch supervisor 已启动");
    loop {
        if let Some(msg) = poll_control(&job_dir) {
            match msg {
                ControlMsg::Stop => {
                    info!(job = %meta.id, "watch supervisor 优雅停止：等待当前任务结束");
                    let _ = engine.request_stop(&meta.id).await;
                    match engine.wait_idle(&meta.id, STOP_IDLE_TIMEOUT).await {
                        Ok(true) => {}
                        Ok(false) => {
                            tracing::warn!(
                                job = %meta.id,
                                "watch 优雅停止超时，改为强制终止"
                            );
                            let _ = engine.shutdown_force(&meta.id).await;
                            let _ = JobMeta::remove(data_dir, JobKind::Watch, &meta.id);
                            let _ = kill_process_tree(std::process::id());
                        }
                        Err(e) => {
                            tracing::warn!(job = %meta.id, error = %e, "watch wait_idle 失败");
                        }
                    }
                    break;
                }
                ControlMsg::StopForce => {
                    info!(job = %meta.id, "watch supervisor 强制停止");
                    let _ = engine.shutdown_force(&meta.id).await;
                    let _ = JobMeta::remove(data_dir, JobKind::Watch, &meta.id);
                    let _ = kill_process_tree(std::process::id());
                    break;
                }
                ControlMsg::RunNow => {
                    if let Err(e) = engine.run_now(&meta.id).await {
                        tracing::warn!(job = %meta.id, error = %e, "watch RUN_NOW 失败");
                    }
                }
                ControlMsg::Status => {
                    let jobs = engine.jobs().await;
                    info!(job = %meta.id, count = jobs.len(), "watch STATUS");
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let _ = engine.unregister(&meta.id).await;
    let _ = JobMeta::remove(data_dir, JobKind::Watch, &meta.id);
    Ok(())
}

#[cfg(feature = "cron")]
pub async fn supervise_cron_job(
    meta: &crate::supervisor::JobMeta,
    store: std::sync::Arc<dyn corex_core::ActionStore>,
    runtime: corex_core::RuntimeConfig,
    data_dir: &std::path::Path,
    io: &crate::run::SupervisorIo,
) -> Result<(), corex_core::EngineError> {
    use crate::cron::{CronEngine, CronJobSpec, bind_cron_engine, effective_cron_timezone};
    use crate::definition::Directive;
    use crate::supervisor::process::kill_process_tree;
    use crate::supervisor::resolve::resolve_cron_expr;
    use crate::supervisor::{ControlMsg, JobKind, JobMeta, poll_control};
    use crate::trigger::find_cron_trigger;
    use std::sync::Arc;
    use std::time::Duration;
    use tracing::info;

    const STOP_IDLE_TIMEOUT: Duration = Duration::from_secs(3_600);

    // 与 watch 同理：指令以库里的那份为准，文件只是老用法的回退。
    let directive = match &io.source {
        Some(source) => source.load(&meta.directive_name)?,
        None => Directive::from_yaml_file(&meta.directive_path)?,
    };
    let engine = CronEngine::new(
        data_dir.to_path_buf(),
        store,
        runtime.clone(),
        io.source.clone(),
        io.history.clone(),
    )
    .await?;
    bind_cron_engine(Arc::clone(&engine));
    let cron = find_cron_trigger(&directive.triggers)?.ok_or_else(|| {
        corex_core::EngineError::other(format!("指令 {} 未声明 cron 触发器", meta.directive_name))
    })?;
    let expr = resolve_cron_expr(&directive, runtime.clone(), &cron.expr)?;
    let timezone = effective_cron_timezone(cron.timezone.as_deref(), &runtime.cron_timezone);
    engine
        .register(CronJobSpec {
            id: meta.id.clone(),
            expr,
            timezone: timezone.clone(),
            directive_path: meta.directive_path.clone(),
            directive_name: meta.directive_name.clone(),
        })
        .await?;
    let job_dir = JobMeta::job_dir(data_dir, JobKind::Cron, &meta.id);
    info!(job = %meta.id, timezone = %timezone, "cron supervisor 已启动");
    loop {
        if let Some(msg) = poll_control(&job_dir) {
            match msg {
                ControlMsg::Stop => {
                    info!(job = %meta.id, "cron supervisor 优雅停止：等待当前任务结束");
                    let _ = engine.request_stop(&meta.id).await;
                    match engine.wait_idle(&meta.id, STOP_IDLE_TIMEOUT).await {
                        Ok(true) => {}
                        Ok(false) => {
                            tracing::warn!(job = %meta.id, "cron 优雅停止超时，改为强制终止");
                            let _ = engine.shutdown_force(&meta.id).await;
                            let _ = JobMeta::remove(data_dir, JobKind::Cron, &meta.id);
                            let _ = kill_process_tree(std::process::id());
                        }
                        Err(e) => {
                            tracing::warn!(job = %meta.id, error = %e, "cron wait_idle 失败");
                        }
                    }
                    break;
                }
                ControlMsg::StopForce => {
                    info!(job = %meta.id, "cron supervisor 强制停止");
                    let _ = engine.shutdown_force(&meta.id).await;
                    let _ = JobMeta::remove(data_dir, JobKind::Cron, &meta.id);
                    let _ = kill_process_tree(std::process::id());
                    break;
                }
                ControlMsg::RunNow => {
                    let _ = engine.run_now(&meta.id).await;
                }
                ControlMsg::Status => {
                    let jobs = engine.jobs().await;
                    info!(job = %meta.id, count = jobs.len(), "cron STATUS");
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let _ = engine.unregister(&meta.id).await;
    let _ = JobMeta::remove(data_dir, JobKind::Cron, &meta.id);
    Ok(())
}
