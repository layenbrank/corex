//! Cron 作业调度引擎。

use super::expr::parse_cron_expr;
use super::tz::{ResolvedCronTz, parse_cron_timezone};
use crate::history::HistorySink;
use crate::run::{DirectiveSource, run_supervised_directive};
use crate::supervisor::JobKind;
use corex_core::{ActionStore, EngineError, RuntimeConfig};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

/// 已注册的 cron 作业元数据。
#[derive(Debug, Clone)]
pub struct CronJobSpec {
    pub id: String,
    pub expr: String,
    /// 生效的时区（`local`、`utc` 或 `±HH:MM`）。
    pub timezone: String,
    pub directive_path: PathBuf,
    pub directive_name: String,
}

struct JobState {
    spec: CronJobSpec,
    is_running: Arc<AtomicBool>,
    uuid: Uuid,
}

/// 触发器与 `cron.schedule` 共用的 cron 调度器。
pub struct CronEngine {
    data_dir: PathBuf,
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    /// 每次触发从哪取指令；`None` 时只能按 `directive_path` 读文件（老行为）。
    source: Option<Arc<dyn DirectiveSource>>,
    /// 执行日志写到哪；`None` 时由 runner 按 `[history]` 配置决定。
    history: Option<Arc<dyn HistorySink>>,
    scheduler: tokio_cron_scheduler::JobScheduler,
    jobs: Mutex<HashMap<String, JobState>>,
}

type CronJobFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

impl CronEngine {
    pub async fn new(
        data_dir: PathBuf,
        store: Arc<dyn ActionStore>,
        runtime: RuntimeConfig,
        source: Option<Arc<dyn DirectiveSource>>,
        history: Option<Arc<dyn HistorySink>>,
    ) -> Result<Arc<Self>, EngineError> {
        let scheduler = tokio_cron_scheduler::JobScheduler::new()
            .await
            .map_err(|e| EngineError::other(format!("cron 调度器初始化失败: {e}")))?;
        scheduler
            .start()
            .await
            .map_err(|e| EngineError::other(format!("cron 调度器启动失败: {e}")))?;
        Ok(Arc::new(Self {
            data_dir,
            store,
            runtime,
            source,
            history,
            scheduler,
            jobs: Mutex::new(HashMap::new()),
        }))
    }

    pub async fn register(&self, spec: CronJobSpec) -> Result<String, EngineError> {
        let parsed = parse_cron_expr(&spec.expr)?;
        let tz = parse_cron_timezone(&spec.timezone)?;
        let job_id = spec.id.clone();
        if self.jobs.lock().await.contains_key(&job_id) {
            self.unregister(&job_id).await?;
        }

        let store = Arc::clone(&self.store);
        let runtime = self.runtime.clone();
        let data_dir = self.data_dir.clone();
        let source = self.source.clone();
        let history = self.history.clone();
        let path = spec.directive_path.clone();
        let name = spec.directive_name.clone();
        let is_running = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&is_running);
        let tz_label = spec.timezone.clone();

        let job = build_timezone_job(parsed.as_str(), tz, move |_uuid, _l| {
            let store = Arc::clone(&store);
            let runtime = runtime.clone();
            let data_dir = data_dir.clone();
            let source = source.clone();
            let history = history.clone();
            let path = path.clone();
            let name = name.clone();
            let flag = Arc::clone(&flag);
            Box::pin(async move {
                if flag
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    warn!(directive = %name, "cron 跳过：上次执行仍在运行");
                    return;
                }
                info!(directive = %name, "cron 触发执行");
                let result = run_supervised_directive(
                    store,
                    runtime,
                    &data_dir,
                    source.as_ref(),
                    history.as_ref(),
                    JobKind::Cron,
                    &name,
                    &path,
                )
                .await;
                if let Err(e) = result {
                    warn!(directive = %name, error = %e, "cron 执行失败");
                }
                flag.store(false, Ordering::SeqCst);
            }) as CronJobFuture
        })?;

        let uuid = self
            .scheduler
            .add(job)
            .await
            .map_err(|e| EngineError::other(format!("cron 注册失败: {e}")))?;

        info!(
            job = %job_id,
            expr = %parsed,
            timezone = %tz_label,
            "cron job 已注册"
        );

        self.jobs.lock().await.insert(
            job_id.clone(),
            JobState {
                spec,
                is_running,
                uuid,
            },
        );
        Ok(job_id)
    }

    pub async fn unregister(&self, job_id: &str) -> Result<(), EngineError> {
        if let Some(state) = self.jobs.lock().await.remove(job_id) {
            // 可能已在 request_stop 里摘过调度，重复 remove 忽略
            let _ = self.scheduler.remove(&state.uuid).await;
        }
        Ok(())
    }

    /// 优雅停止：先从调度器摘掉（不再接新 tick），保留状态供 wait_idle。
    pub async fn request_stop(&self, job_id: &str) -> Result<(), EngineError> {
        let jobs = self.jobs.lock().await;
        let state = jobs
            .get(job_id)
            .ok_or_else(|| EngineError::other(format!("cron job 未找到: {job_id}")))?;
        let _ = self.scheduler.remove(&state.uuid).await;
        Ok(())
    }

    /// 等到没有进行中的触发（或 job 已卸）。超时返回 `false`，由调用方改走强制停。
    pub async fn wait_idle(
        &self,
        job_id: &str,
        timeout: std::time::Duration,
    ) -> Result<bool, EngineError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let running = {
                let jobs = self.jobs.lock().await;
                match jobs.get(job_id) {
                    Some(state) => state.is_running.load(Ordering::SeqCst),
                    None => false,
                }
            };
            if !running {
                return Ok(true);
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// 移除已调度作业，并停止接受新的 cron 触发。
    pub async fn shutdown_force(&self, job_id: &str) -> Result<(), EngineError> {
        self.unregister(job_id).await
    }

    pub async fn run_now(&self, job_id: &str) -> Result<(), EngineError> {
        let jobs = self.jobs.lock().await;
        let state = jobs
            .get(job_id)
            .ok_or_else(|| EngineError::other(format!("cron job 未找到: {job_id}")))?;
        if state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(EngineError::other("job 正在运行"));
        }
        let store = Arc::clone(&self.store);
        let runtime = self.runtime.clone();
        let data_dir = self.data_dir.clone();
        let source = self.source.clone();
        let history = self.history.clone();
        let path = state.spec.directive_path.clone();
        let name = state.spec.directive_name.clone();
        let flag = Arc::clone(&state.is_running);
        drop(jobs);
        tokio::spawn(async move {
            info!(directive = %name, "cron RUN_NOW");
            let _ = run_supervised_directive(
                store,
                runtime,
                &data_dir,
                source.as_ref(),
                history.as_ref(),
                JobKind::Cron,
                &name,
                &path,
            )
            .await;
            flag.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    pub async fn jobs(&self) -> Vec<CronJobSpec> {
        self.jobs
            .lock()
            .await
            .values()
            .map(|j| j.spec.clone())
            .collect()
    }

    pub async fn has_job(&self, job_id: &str) -> bool {
        self.jobs.lock().await.contains_key(job_id)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

fn build_timezone_job<F>(
    expr: &str,
    tz: ResolvedCronTz,
    run: F,
) -> Result<tokio_cron_scheduler::Job, EngineError>
where
    F: FnMut(Uuid, tokio_cron_scheduler::JobScheduler) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync
        + 'static,
{
    let job = match tz {
        ResolvedCronTz::Utc => tokio_cron_scheduler::Job::new_async_tz(expr, chrono::Utc, run),
        ResolvedCronTz::Local => tokio_cron_scheduler::Job::new_async_tz(expr, chrono::Local, run),
        ResolvedCronTz::Fixed(offset) => tokio_cron_scheduler::Job::new_async_tz(expr, offset, run),
    };
    job.map_err(|e| EngineError::ParseError(format!("cron expr 无效: {e}")))
}
