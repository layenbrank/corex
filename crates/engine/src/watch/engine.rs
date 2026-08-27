//! File watch engine (notify + debounce + cooldown).

use super::filter::path_matches;
use crate::run::run_directive_file;
use crate::trigger::WatchConfig;
use corex_core::{ActionStore, EngineError, RuntimeConfig};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use notify::RecursiveMode;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Active watch job.
#[derive(Debug, Clone)]
pub struct WatchJobSpec {
    pub id: String,
    pub directive_path: PathBuf,
    pub directive_name: String,
    pub config: WatchConfig,
}

struct WatchState {
    spec: WatchJobSpec,
    is_running: Arc<AtomicBool>,
    _debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
}

/// Directory/file watcher with debounce and cooldown.
pub struct WatchEngine {
    data_dir: PathBuf,
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    jobs: Mutex<HashMap<String, WatchState>>,
}

impl WatchEngine {
    pub fn new(
        data_dir: PathBuf,
        store: Arc<dyn ActionStore>,
        runtime: RuntimeConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            data_dir,
            store,
            runtime,
            jobs: Mutex::new(HashMap::new()),
        })
    }

    pub async fn register(&self, spec: WatchJobSpec) -> Result<String, EngineError> {
        let job_id = spec.id.clone();
        if self.jobs.lock().await.contains_key(&job_id) {
            self.unregister(&job_id).await?;
        }

        let store = Arc::clone(&self.store);
        let runtime = self.runtime.clone();
        let data_dir = self.data_dir.clone();
        let path = spec.directive_path.clone();
        let name = spec.directive_name.clone();
        let cfg = spec.config.clone();
        let is_running = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&is_running);

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let cooldown_ms = cfg.cooldown_ms;
        let worker_store = Arc::clone(&store);
        let worker_runtime = runtime.clone();
        let worker_data = data_dir.clone();
        let worker_path = path.clone();
        let worker_name = name.clone();
        let worker_flag = Arc::clone(&flag);
        tokio::spawn(async move {
            let mut last_run: Option<Instant> = None;
            while rx.recv().await.is_some() {
                if let Some(t) = last_run {
                    if t.elapsed() < Duration::from_millis(cooldown_ms) {
                        continue;
                    }
                }
                last_run = Some(Instant::now());
                if worker_flag
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_err()
                {
                    continue;
                }
                info!(directive = %worker_name, "watch 触发执行");
                let result = run_directive_file(
                    Arc::clone(&worker_store),
                    worker_runtime.clone(),
                    worker_data.clone(),
                    &worker_path,
                )
                .await;
                if let Err(e) = result {
                    warn!(directive = %worker_name, error = %e, "watch 执行失败");
                }
                worker_flag.store(false, Ordering::SeqCst);
            }
        });

        let includes_cb = cfg.includes.clone();
        let excludes_cb = cfg.excludes.clone();
        let tx_cb = tx.clone();
        let debounce_ms = cfg.debounce_ms;
        let mut debouncer = new_debouncer(
            Duration::from_millis(debounce_ms),
            None,
            move |result: DebounceEventResult| {
                let Ok(events) = result else {
                    return;
                };
                for debounced in events {
                    let mut matched = false;
                    for event in debounced.event.paths {
                        let rel = event
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if path_matches(&rel, &includes_cb, &excludes_cb) {
                            matched = true;
                            break;
                        }
                    }
                    if matched {
                        let _ = tx_cb.send(());
                    }
                }
            },
        )
        .map_err(|e| EngineError::other(format!("watch debouncer 初始化失败: {e}")))?;

        for raw in &spec.config.paths {
            let p = resolve_watch_path(raw);
            if !p.exists() {
                warn!(path = %p.display(), "watch 路径不存在，跳过");
                continue;
            }
            let mode = if p.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            debouncer
                .watch(&p, mode)
                .map_err(|e| EngineError::other(format!("watch 注册失败 {}: {e}", p.display())))?;
        }

        self.jobs.lock().await.insert(
            job_id.clone(),
            WatchState {
                spec,
                is_running,
                _debouncer: debouncer,
            },
        );
        Ok(job_id)
    }

    pub async fn unregister(&self, job_id: &str) -> Result<(), EngineError> {
        self.jobs.lock().await.remove(job_id);
        Ok(())
    }

    pub async fn run_now(&self, job_id: &str) -> Result<(), EngineError> {
        let jobs = self.jobs.lock().await;
        let state = jobs
            .get(job_id)
            .ok_or_else(|| EngineError::other(format!("watch job 未找到: {job_id}")))?;
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
            info!(directive = %name, "watch RUN_NOW");
            let _ = run_directive_file(store, runtime, data_dir, &path).await;
            flag.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    pub async fn list_jobs(&self) -> Vec<WatchJobSpec> {
        self.jobs
            .lock()
            .await
            .values()
            .map(|j| j.spec.clone())
            .collect()
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

fn resolve_watch_path(raw: &str) -> PathBuf {
    PathBuf::from(raw)
}
