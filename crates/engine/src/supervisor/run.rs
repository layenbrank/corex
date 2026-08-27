//! Supervisor run loops for watch and cron.

#[cfg(feature = "watch")]
pub async fn supervise_watch_job(
    meta: &crate::supervisor::JobMeta,
    store: std::sync::Arc<dyn corex_core::ActionStore>,
    runtime: corex_core::RuntimeConfig,
    data_dir: &std::path::Path,
) -> Result<(), corex_core::EngineError> {
    use crate::definition::Directive;
    use crate::supervisor::{poll_control, ControlMsg, JobKind, JobMeta};
    use crate::trigger::find_watch_trigger;
    use crate::watch::{WatchEngine, WatchJobSpec};
    use std::time::Duration;
    use tracing::info;

    let directive = Directive::from_yaml_file(&meta.directive_path)?;
    let engine = WatchEngine::new(data_dir.to_path_buf(), store, runtime);
    let watch = find_watch_trigger(&directive.triggers)?.ok_or_else(|| {
        corex_core::EngineError::other(format!(
            "指令 {} 未声明 watch 触发器",
            meta.directive_name
        ))
    })?;
    engine
        .register(WatchJobSpec {
            id: meta.id.clone(),
            directive_path: meta.directive_path.clone(),
            directive_name: meta.directive_name.clone(),
            config: watch,
        })
        .await?;
    let job_dir = JobMeta::job_dir(data_dir, JobKind::Watch, &meta.id);
    info!(job = %meta.id, "watch supervisor 已启动");
    loop {
        if let Some(msg) = poll_control(&job_dir) {
            match msg {
                ControlMsg::Stop => {
                    info!(job = %meta.id, "watch supervisor 停止");
                    break;
                }
                ControlMsg::RunNow => {
                    let _ = engine.run_now(&meta.id).await;
                }
                ControlMsg::Status => {
                    let jobs = engine.list_jobs().await;
                    info!(job = %meta.id, count = jobs.len(), "watch STATUS");
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(())
}

#[cfg(feature = "cron")]
pub async fn supervise_cron_job(
    meta: &crate::supervisor::JobMeta,
    store: std::sync::Arc<dyn corex_core::ActionStore>,
    runtime: corex_core::RuntimeConfig,
    data_dir: &std::path::Path,
) -> Result<(), corex_core::EngineError> {
    use crate::cron::{bind_cron_engine, CronEngine, CronJobSpec};
    use crate::definition::Directive;
    use crate::supervisor::{poll_control, ControlMsg, JobKind, JobMeta};
    use crate::trigger::find_cron_trigger;
    use std::sync::Arc;
    use std::time::Duration;
    use tracing::info;

    let directive = Directive::from_yaml_file(&meta.directive_path)?;
    let engine = CronEngine::new(data_dir.to_path_buf(), store, runtime).await?;
    bind_cron_engine(Arc::clone(&engine));
    let cron = find_cron_trigger(&directive.triggers)?.ok_or_else(|| {
        corex_core::EngineError::other(format!(
            "指令 {} 未声明 cron 触发器",
            meta.directive_name
        ))
    })?;
    engine
        .register(CronJobSpec {
            id: meta.id.clone(),
            expr: cron.expr,
            directive_path: meta.directive_path.clone(),
            directive_name: meta.directive_name.clone(),
        })
        .await?;
    let job_dir = JobMeta::job_dir(data_dir, JobKind::Cron, &meta.id);
    info!(job = %meta.id, "cron supervisor 已启动");
    loop {
        if let Some(msg) = poll_control(&job_dir) {
            match msg {
                ControlMsg::Stop => {
                    info!(job = %meta.id, "cron supervisor 停止");
                    break;
                }
                ControlMsg::RunNow => {
                    let _ = engine.run_now(&meta.id).await;
                }
                ControlMsg::Status => {
                    let jobs = engine.list_jobs().await;
                    info!(job = %meta.id, count = jobs.len(), "cron STATUS");
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Ok(())
}
