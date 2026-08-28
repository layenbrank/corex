//! File watch engine (notify + debounce + cooldown).

use super::event::{EventAction, EventFilter, classify_event};
use super::filter::{WatchFilter, watch_relative_path};
use crate::run::run_directive_file;
use crate::trigger::WatchConfig;
use corex_core::{ActionStore, EngineError, RuntimeConfig};
use notify::{Config as NotifyConfig, PollWatcher, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    DebounceEventHandler, DebounceEventResult, Debouncer, FileIdMap, new_debouncer_opt,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};

const TRIGGER_CHANNEL_CAP: usize = 64;
const REMOUNT_POLL_MS: u64 = 500;
const REMOUNT_TIMEOUT_MS: u64 = 60_000;
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Active watch job.
#[derive(Debug, Clone)]
pub struct WatchJobSpec {
    pub id: String,
    pub directive_path: PathBuf,
    pub directive_name: String,
    pub config: WatchConfig,
}

enum JobDebouncer {
    Recommended(Debouncer<RecommendedWatcher, FileIdMap>),
    Poll(Debouncer<PollWatcher, FileIdMap>),
}

impl JobDebouncer {
    fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
        match self {
            Self::Recommended(d) => d.watch(path, mode),
            Self::Poll(d) => d.watch(path, mode),
        }
    }
}

enum RemountCmd {
    All,
    Root(PathBuf),
}

struct WatchState {
    spec: WatchJobSpec,
    is_running: Arc<AtomicBool>,
    worker_abort: tokio::task::AbortHandle,
    remount_abort: tokio::task::AbortHandle,
    _worker_tx: tokio::sync::mpsc::Sender<()>,
    _debouncer: Arc<Mutex<Option<JobDebouncer>>>,
}

struct DebounceHandler {
    ignore_initial: Arc<AtomicBool>,
    filter: WatchFilter,
    event_filter: EventFilter,
    watch_roots: Vec<String>,
    mount_roots: Vec<PathBuf>,
    trigger_tx: tokio::sync::mpsc::Sender<()>,
    remount_tx: tokio::sync::mpsc::UnboundedSender<RemountCmd>,
}

impl DebounceEventHandler for DebounceHandler {
    fn handle_event(&mut self, result: DebounceEventResult) {
        if self.ignore_initial.load(Ordering::SeqCst) {
            return;
        }
        match result {
            Ok(events) => {
                for debounced in events {
                    match classify_event(&debounced.event, &self.mount_roots, &self.event_filter) {
                        EventAction::Remount => {
                            let _ = self.remount_tx.send(RemountCmd::All);
                        }
                        EventAction::RemountRoot(path) => {
                            let _ = self.remount_tx.send(RemountCmd::Root(path));
                        }
                        EventAction::Skip => {}
                        EventAction::Trigger => {
                            let matched = debounced.event.paths.iter().any(|path| {
                                let rel = watch_relative_path(path, &self.watch_roots);
                                self.filter.matches(&rel)
                            });
                            if matched {
                                let _ = self.trigger_tx.try_send(());
                            }
                        }
                    }
                }
            }
            Err(errors) => {
                for e in errors {
                    warn!(error = %e, "watch debouncer 错误，尝试重挂");
                }
                let _ = self.remount_tx.send(RemountCmd::All);
            }
        }
    }
}

/// Directory/file watcher with debounce and cooldown.
pub struct WatchEngine {
    data_dir: PathBuf,
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    jobs: AsyncMutex<HashMap<String, WatchState>>,
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
            jobs: AsyncMutex::new(HashMap::new()),
        })
    }

    pub async fn register(&self, spec: WatchJobSpec) -> Result<String, EngineError> {
        let job_id = spec.id.clone();
        if self.jobs.lock().await.contains_key(&job_id) {
            self.unregister(&job_id).await?;
        }

        let cfg = spec.config.clone();
        let mount_specs = resolve_roots(&cfg);
        let mount_paths: Vec<PathBuf> = mount_specs
            .iter()
            .map(|raw| resolve_watch_path(raw))
            .collect();
        let watch_roots_str = cfg.paths.clone();

        let is_running = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&is_running);

        let (trigger_tx, mut trigger_rx) = tokio::sync::mpsc::channel::<()>(TRIGGER_CHANNEL_CAP);
        let (remount_tx, mut remount_rx) = tokio::sync::mpsc::unbounded_channel::<RemountCmd>();

        let cooldown_ms = cfg.cooldown_ms;
        let worker_store = Arc::clone(&self.store);
        let worker_runtime = self.runtime.clone();
        let worker_data = self.data_dir.clone();
        let worker_path = spec.directive_path.clone();
        let worker_name = spec.directive_name.clone();
        let worker_flag = Arc::clone(&flag);

        let worker = tokio::spawn(async move {
            let mut last_run: Option<Instant> = None;
            let mut pending = false;
            loop {
                if !pending {
                    if trigger_rx.recv().await.is_none() {
                        break;
                    }
                }
                // Coalesce queued triggers into a single pending run.
                while trigger_rx.try_recv().is_ok() {}
                pending = false;

                // Wait out another runner (e.g. RUN_NOW) without busy-spinning.
                while worker_flag.load(Ordering::SeqCst) {
                    pending = true;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                if pending {
                    while trigger_rx.try_recv().is_ok() {}
                    pending = false;
                }

                if let Some(t) = last_run {
                    let elapsed = t.elapsed();
                    let cooldown = Duration::from_millis(cooldown_ms);
                    if elapsed < cooldown {
                        tokio::time::sleep(cooldown - elapsed).await;
                        while trigger_rx.try_recv().is_ok() {}
                    }
                }

                last_run = Some(Instant::now());
                worker_flag.store(true, Ordering::SeqCst);
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

                // Pipeline 期间到达的事件：结束后补触发一次（仍走 cooldown）。
                if trigger_rx.try_recv().is_ok() {
                    while trigger_rx.try_recv().is_ok() {}
                    pending = true;
                }
            }
        });
        let worker_abort = worker.abort_handle();

        let ignore_initial = Arc::new(AtomicBool::new(true));
        let debouncer_slot: Arc<Mutex<Option<JobDebouncer>>> = Arc::new(Mutex::new(None));

        let handler = DebounceHandler {
            ignore_initial: Arc::clone(&ignore_initial),
            filter: WatchFilter::new(&cfg.includes, &cfg.excludes),
            event_filter: EventFilter::from_events(&cfg.events),
            watch_roots: watch_roots_str,
            mount_roots: mount_paths.clone(),
            trigger_tx: trigger_tx.clone(),
            remount_tx: remount_tx.clone(),
        };

        let debounce_ms = cfg.debounce_ms;
        let tick_rate = Duration::from_millis(debounce_ms.max(4) / 4);
        let notify_cfg = if cfg.poll {
            NotifyConfig::default().with_poll_interval(POLL_INTERVAL)
        } else {
            NotifyConfig::default()
        };
        let debouncer = if cfg.poll {
            JobDebouncer::Poll(
                new_debouncer_opt(
                    Duration::from_millis(debounce_ms),
                    Some(tick_rate),
                    handler,
                    FileIdMap::new(),
                    notify_cfg,
                )
                .map_err(|e| EngineError::other(format!("watch debouncer 初始化失败: {e}")))?,
            )
        } else {
            JobDebouncer::Recommended(
                new_debouncer_opt(
                    Duration::from_millis(debounce_ms),
                    Some(tick_rate),
                    handler,
                    FileIdMap::new(),
                    notify_cfg,
                )
                .map_err(|e| EngineError::other(format!("watch debouncer 初始化失败: {e}")))?,
            )
        };

        *debouncer_slot
            .lock()
            .map_err(|e| EngineError::other(format!("watch debouncer 锁失败: {e}")))? =
            Some(debouncer);

        mount_all(
            debouncer_slot
                .lock()
                .map_err(|e| EngineError::other(format!("watch debouncer 锁失败: {e}")))?
                .as_mut()
                .ok_or_else(|| EngineError::other("watch debouncer 未初始化"))?,
            &mount_specs,
        )?;

        ignore_initial.store(false, Ordering::SeqCst);

        let debouncer_for_remount = Arc::clone(&debouncer_slot);
        let remount_specs = mount_specs.clone();
        let remount_task = tokio::spawn(async move {
            let mut backoff_ms = 1000u64;
            while let Some(cmd) = remount_rx.recv().await {
                match cmd {
                    RemountCmd::All => {
                        let remount_result =
                            debouncer_for_remount.lock().ok().and_then(|mut guard| {
                                guard.as_mut().map(|d| mount_all(d, &remount_specs))
                            });
                        match remount_result {
                            Some(Ok(())) => {
                                info!("watch 路径已重新挂载");
                                backoff_ms = 1000;
                            }
                            Some(Err(e)) => {
                                warn!(error = %e, "watch remount 失败");
                                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                backoff_ms = (backoff_ms * 2).min(30_000);
                            }
                            None => {}
                        }
                    }
                    RemountCmd::Root(removed) => {
                        let root_str = removed.to_string_lossy().into_owned();
                        let specs: Vec<String> = remount_specs
                            .iter()
                            .filter(|s| {
                                let p = resolve_watch_path(s);
                                p.components().eq(removed.components()) || p.starts_with(&removed)
                            })
                            .cloned()
                            .collect();
                        let wait_specs = if specs.is_empty() {
                            vec![root_str]
                        } else {
                            specs
                        };
                        let deadline = Instant::now() + Duration::from_millis(REMOUNT_TIMEOUT_MS);
                        loop {
                            if Instant::now() >= deadline {
                                warn!(path = %removed.display(), "watch 等待路径重现超时");
                                break;
                            }
                            if wait_specs.iter().any(|s| resolve_watch_path(s).exists()) {
                                let _ = debouncer_for_remount.lock().ok().and_then(|mut guard| {
                                    guard.as_mut().map(|d| mount_all(d, &wait_specs))
                                });
                                info!(path = %removed.display(), "watch 路径已重新挂载");
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(REMOUNT_POLL_MS)).await;
                        }
                    }
                }
            }
        });

        self.jobs.lock().await.insert(
            job_id.clone(),
            WatchState {
                spec,
                is_running,
                worker_abort,
                remount_abort: remount_task.abort_handle(),
                _worker_tx: trigger_tx,
                _debouncer: debouncer_slot,
            },
        );

        Ok(job_id)
    }

    pub async fn unregister(&self, job_id: &str) -> Result<(), EngineError> {
        if let Some(state) = self.jobs.lock().await.remove(job_id) {
            state.worker_abort.abort();
            state.remount_abort.abort();
            if let Ok(mut guard) = state._debouncer.lock() {
                *guard = None;
            }
        }
        Ok(())
    }

    pub async fn shutdown_force(&self, job_id: &str) -> Result<(), EngineError> {
        self.unregister(job_id).await
    }

    pub async fn run_now(&self, job_id: &str) -> Result<(), EngineError> {
        let jobs = self.jobs.lock().await;
        let state = jobs
            .get(job_id)
            .ok_or_else(|| EngineError::other(format!("watch job 未找到: {job_id}")))?;
        if state.is_running.load(Ordering::SeqCst) {
            return Err(EngineError::other("job 正在运行"));
        }
        state.is_running.store(true, Ordering::SeqCst);
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

fn mount_all(debouncer: &mut JobDebouncer, roots: &[String]) -> Result<(), EngineError> {
    for raw in roots {
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
    Ok(())
}

/// Narrow mount paths when includes are simple directory names.
pub fn resolve_roots(config: &WatchConfig) -> Vec<String> {
    let narrow = !config.includes.is_empty()
        && config
            .includes
            .iter()
            .all(|inc| is_simple_dir(inc.as_str()));
    if narrow {
        let mut out = Vec::new();
        for base in &config.paths {
            for inc in &config.includes {
                out.push(
                    PathBuf::from(base)
                        .join(inc)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        out
    } else {
        config.paths.clone()
    }
}

fn is_simple_dir(name: &str) -> bool {
    !name.is_empty() && !name.contains(['*', '?', '[', '/', '\\'])
}

fn resolve_watch_path(raw: &str) -> PathBuf {
    PathBuf::from(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_roots_narrows_simple_includes() {
        let cfg = WatchConfig {
            paths: vec!["/proj".into()],
            includes: vec!["src".into(), "templates".into()],
            excludes: vec![],
            debounce_ms: 300,
            cooldown_ms: 1000,
            immediate: false,
            poll: false,
            events: vec![],
        };
        let roots = resolve_roots(&cfg);
        assert_eq!(roots, vec!["/proj/src", "/proj/templates"]);
    }

    #[test]
    fn resolve_roots_keeps_paths_for_glob_includes() {
        let cfg = WatchConfig {
            paths: vec!["/proj".into()],
            includes: vec!["src/**".into()],
            excludes: vec![],
            debounce_ms: 300,
            cooldown_ms: 1000,
            immediate: false,
            poll: false,
            events: vec![],
        };
        assert_eq!(resolve_roots(&cfg), vec!["/proj"]);
    }
}
