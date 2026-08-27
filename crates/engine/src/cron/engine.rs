//! Cron job scheduling engine.

use super::expr::parse_cron_expr;
use crate::run::run_directive_file;
use corex_core::{ActionStore, EngineError, RuntimeConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

/// Registered cron job metadata.
#[derive(Debug, Clone)]
pub struct CronJobSpec {
    pub id: String,
    pub expr: String,
    pub directive_path: PathBuf,
    pub directive_name: String,
}

struct JobState {
    spec: CronJobSpec,
    is_running: Arc<AtomicBool>,
    uuid: Uuid,
}

/// Shared cron scheduler used by triggers and `cron.schedule`.
pub struct CronEngine {
    data_dir: PathBuf,
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    scheduler: tokio_cron_scheduler::JobScheduler,
    jobs: Mutex<HashMap<String, JobState>>,
}

impl CronEngine {
    pub async fn new(
        data_dir: PathBuf,
        store: Arc<dyn ActionStore>,
        runtime: RuntimeConfig,
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
            scheduler,
            jobs: Mutex::new(HashMap::new()),
        }))
    }

    pub async fn register(&self, spec: CronJobSpec) -> Result<String, EngineError> {
        let parsed = parse_cron_expr(&spec.expr)?;
        let job_id = spec.id.clone();
        if self.jobs.lock().await.contains_key(&job_id) {
            self.unregister(&job_id).await?;
        }

        let store = Arc::clone(&self.store);
        let runtime = self.runtime.clone();
        let data_dir = self.data_dir.clone();
        let path = spec.directive_path.clone();
        let name = spec.directive_name.clone();
        let is_running = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&is_running);

        let job = tokio_cron_scheduler::Job::new_async(parsed.as_str(), move |_uuid, _l| {
            let store = Arc::clone(&store);
            let runtime = runtime.clone();
            let data_dir = data_dir.clone();
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
                let result =
                    run_directive_file(store, runtime, data_dir, &path).await;
                if let Err(e) = result {
                    warn!(directive = %name, error = %e, "cron 执行失败");
                }
                flag.store(false, Ordering::SeqCst);
            })
        })
        .map_err(|e| EngineError::ParseError(format!("cron expr 无效: {e}")))?;

        let uuid = self
            .scheduler
            .add(job)
            .await
            .map_err(|e| EngineError::other(format!("cron 注册失败: {e}")))?;

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
            self.scheduler
                .remove(&state.uuid)
                .await
                .map_err(|e| EngineError::other(format!("cron 移除失败: {e}")))?;
        }
        Ok(())
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
        let path = state.spec.directive_path.clone();
        let name = state.spec.directive_name.clone();
        let flag = Arc::clone(&state.is_running);
        drop(jobs);
        tokio::spawn(async move {
            info!(directive = %name, "cron RUN_NOW");
            let _ = run_directive_file(store, runtime, data_dir, &path).await;
            flag.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    pub async fn list_jobs(&self) -> Vec<CronJobSpec> {
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
