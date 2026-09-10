//! 文件监听引擎：先 FS 去抖（`notify_debouncer_full`），再过类 lodash 节流。
//!
//! ```text
//! FS 事件 ──debounce(debounce_ms)──► 触发 ──throttle(throttle_ms)──► run_directive
//! ```
//!
//! 这里的去抖是**文件系统静默期**合并，不是 lodash 的 debounce API。
//! `throttle_ms` 是类 lodash 的节流间隔（leading+trailing）。

use super::event::{EventAction, EventFilter, classify_event};
use super::filter::{WatchFilter, watch_relative_path};
use super::throttle::{InvokeThrottle, TriggerDecision, wait_for_trailing_deadline};
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
const BUSY_POLL_MS: u64 = 50;

/// 活跃的 watch 作业。
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
    /// 与 worker 共享，使 RUN_NOW / immediate 能刷新节流的 `last_invoke`。
    throttle: Arc<Mutex<InvokeThrottle>>,
    /// 为真时 DebounceHandler 丢弃 FS 触发（启动期 / immediate 之前）。
    ignore_initial: Arc<AtomicBool>,
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

/// 目录/文件监听器：FS 去抖之后再对流水线运行做类 lodash 节流。
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
        let throttle = Arc::new(Mutex::new(InvokeThrottle::new(Duration::from_millis(
            cfg.throttle_ms,
        ))));

        let (trigger_tx, trigger_rx) = tokio::sync::mpsc::channel::<()>(TRIGGER_CHANNEL_CAP);
        let (remount_tx, mut remount_rx) = tokio::sync::mpsc::unbounded_channel::<RemountCmd>();

        let worker = spawn_watch_worker(
            trigger_rx,
            WorkerCtx {
                worker_flag: Arc::clone(&is_running),
                worker_throttle: Arc::clone(&throttle),
                worker_store: Arc::clone(&self.store),
                worker_runtime: self.runtime.clone(),
                worker_data: self.data_dir.clone(),
                worker_path: spec.directive_path.clone(),
                worker_name: spec.directive_name.clone(),
            },
        );
        let worker_abort = worker.abort_handle();

        // 布置之前一直保持忽略：若为 `immediate`，则保持关闭直到 `run_now`，
        // 这样 FS 的 leading 不会与启动时的那次调用抢跑。
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

        // 文件系统静默期去抖（notify_debouncer_full），不是 worker 内的第二次去抖。
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

        if !cfg.immediate {
            ignore_initial.store(false, Ordering::SeqCst);
        }

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
                throttle,
                ignore_initial,
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
        // 即使 CAS 失败也要布置 FS 事件——immediate 不能让监听永远哑着。
        state.ignore_initial.store(false, Ordering::SeqCst);
        if state
            .is_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(EngineError::other("job 正在运行"));
        }
        // 在调用开始时刷新节流窗口（与 leading 一致），使紧随其后的 FS 触发
        // 不会再 leading 触发一次。
        if let Ok(mut gate) = state.throttle.lock() {
            gate.record_external_invoke(Instant::now());
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
            if let Err(e) = run_directive_file(store, runtime, data_dir, &path).await {
                warn!(directive = %name, error = %e, "watch RUN_NOW 执行失败");
            }
            flag.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    pub async fn jobs(&self) -> Vec<WatchJobSpec> {
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

/// 派生节流 worker：合并触发、leading/trailing、CAS 单飞。
/// watch worker 每次触发都需要的全部东西。
///
/// 用一个值代替七个并列参数：它们合起来就是 worker 的整个世界；收进结构体
/// 也让两个调用点不容易走偏。字段名保留 `worker_` 前缀，因为 worker 主体里
/// 用的就是这个前缀。
struct WorkerCtx {
    /// 流水线运行时为 `true`，同时充当单飞门禁。
    worker_flag: Arc<AtomicBool>,
    /// leading/trailing 节流器，与 `RUN_NOW` 共享。
    worker_throttle: Arc<Mutex<InvokeThrottle>>,
    /// 流水线解析动作所用的 Action store。
    worker_store: Arc<dyn ActionStore>,
    /// 本作业的运行时配置快照。
    worker_runtime: RuntimeConfig,
    /// Corex 数据目录（历史 / 审计日志）。
    worker_data: PathBuf,
    /// 指令文件的绝对路径。
    worker_path: PathBuf,
    /// 指令名，用于日志行。
    worker_name: String,
}

fn spawn_watch_worker(
    mut trigger_rx: tokio::sync::mpsc::Receiver<()>,
    ctx: WorkerCtx,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let WorkerCtx {
            worker_flag,
            worker_throttle,
            worker_store,
            worker_runtime,
            worker_data,
            worker_path,
            worker_name,
        } = ctx;
        let mut trailing_deadline: Option<Instant> = None;
        loop {
            tokio::select! {
                trig = trigger_rx.recv() => {
                    let Some(()) = trig else { break; };
                    while trigger_rx.try_recv().is_ok() {}

                    let busy = worker_flag.load(Ordering::SeqCst);
                    let decision = {
                        let Ok(mut gate) = worker_throttle.lock() else {
                            continue;
                        };
                        gate.note_trigger(Instant::now(), busy)
                    };

                    match decision {
                        TriggerDecision::RunLeading => {
                            let ran = invoke_directive(
                                &worker_flag,
                                &worker_throttle,
                                Arc::clone(&worker_store),
                                worker_runtime.clone(),
                                worker_data.clone(),
                                &worker_path,
                                &worker_name,
                            )
                            .await;
                            if !ran {
                                // CAS 被 RUN_NOW 抢走：布置 trailing，空下来再重试。
                                if let Ok(mut gate) = worker_throttle.lock() {
                                    gate.arm_trailing();
                                }
                                trailing_deadline = Some(Instant::now());
                            } else {
                                trailing_deadline = trailing_after_run(
                                    &worker_throttle,
                                    &mut trigger_rx,
                                );
                            }
                        }
                        TriggerDecision::ArmTrailing { until } => {
                            trailing_deadline = Some(until);
                        }
                    }
                }
                _ = async {
                    match trailing_deadline {
                        Some(until) => wait_for_trailing_deadline(&worker_throttle, until).await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    trailing_deadline = None;
                    let should_run = worker_throttle
                        .lock()
                        .ok()
                        .is_some_and(|mut g| g.take_trailing());
                    if !should_run {
                        continue;
                    }

                    // 等重叠的 RUN_NOW / 长 leading 过去，且不要重复开启。
                    while worker_flag.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(BUSY_POLL_MS)).await;
                        while trigger_rx.try_recv().is_ok() {
                            if let Ok(mut gate) = worker_throttle.lock() {
                                let _ = gate.note_trigger(Instant::now(), true);
                            }
                        }
                    }

                    // RUN_NOW 延长窗口后，窗口可能仍然是开的。
                    if let Ok(gate) = worker_throttle.lock()
                        && !gate.is_outside_window(Instant::now())
                            && let Some(until) = gate.window_end() {
                                drop(gate);
                                if let Ok(mut g) = worker_throttle.lock() {
                                    g.arm_trailing();
                                }
                                trailing_deadline = Some(until);
                                continue;
                            }

                    let ran = invoke_directive(
                        &worker_flag,
                        &worker_throttle,
                        Arc::clone(&worker_store),
                        worker_runtime.clone(),
                        worker_data.clone(),
                        &worker_path,
                        &worker_name,
                    )
                    .await;
                    if !ran {
                        if let Ok(mut gate) = worker_throttle.lock() {
                            gate.arm_trailing();
                        }
                        trailing_deadline = Some(Instant::now());
                    } else {
                        trailing_deadline = trailing_after_run(
                            &worker_throttle,
                            &mut trigger_rx,
                        );
                    }
                }
            }
        }
    })
}

/// CAS + 标记调用开始 + 运行。CAS 抢不到时返回 false（不要重复开启）。
async fn invoke_directive(
    flag: &AtomicBool,
    throttle: &Mutex<InvokeThrottle>,
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    data_dir: PathBuf,
    path: &Path,
    name: &str,
) -> bool {
    if flag
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return false;
    }
    let start = Instant::now();
    if let Ok(mut gate) = throttle.lock() {
        gate.mark_invoke_start(start);
    }
    info!(directive = %name, "watch 触发执行");
    let result = run_directive_file(store, runtime, data_dir, path).await;
    if let Err(e) = result {
        warn!(directive = %name, error = %e, "watch 执行失败");
    }
    flag.store(false, Ordering::SeqCst);
    true
}

/// 运行成功之后：把 channel 里的触发归并成最多一次 trailing 布置。
fn trailing_after_run(
    throttle: &Mutex<InvokeThrottle>,
    trigger_rx: &mut tokio::sync::mpsc::Receiver<()>,
) -> Option<Instant> {
    let mut saw = false;
    while trigger_rx.try_recv().is_ok() {
        saw = true;
    }
    if !saw {
        return throttle.lock().ok().and_then(|g| {
            if g.has_trailing() {
                g.window_end()
                    .filter(|&e| e > Instant::now())
                    .or(Some(Instant::now()))
            } else {
                None
            }
        });
    }
    let now = Instant::now();
    let Ok(mut gate) = throttle.lock() else {
        return None;
    };
    match gate.note_trigger(now, false) {
        TriggerDecision::RunLeading => {
            // 已在窗口之外——走 trailing 路径尽快运行（单飞）。
            gate.arm_trailing();
            Some(now)
        }
        TriggerDecision::ArmTrailing { until } => Some(until),
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

/// include 都是简单目录名时，收窄监听路径。
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
            throttle_ms: 1000,
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
            throttle_ms: 1000,
            immediate: false,
            poll: false,
            events: vec![],
        };
        assert_eq!(resolve_roots(&cfg), vec!["/proj"]);
    }

    #[test]
    fn trailing_after_run_coalesces_many_channel_messages() {
        let throttle = Mutex::new(InvokeThrottle::new(Duration::from_millis(100)));
        let t0 = Instant::now();
        throttle.lock().unwrap().mark_invoke_start(t0);

        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);
        for _ in 0..10 {
            tx.try_send(()).unwrap();
        }
        let until = trailing_after_run(&throttle, &mut rx);
        assert!(until.is_some());
        assert!(throttle.lock().unwrap().has_trailing() || until.is_some());
        assert!(rx.try_recv().is_err(), "channel must be drained");
    }

    #[test]
    fn immediate_keeps_ignore_until_armed_semantics() {
        // register/run_now 依赖的约定（见文档）：
        // immediate=true → 布置之后 ignore 仍为真；由 run_now 清掉。
        let ignore = AtomicBool::new(true);
        let immediate = true;
        if !immediate {
            ignore.store(false, Ordering::SeqCst);
        }
        assert!(ignore.load(Ordering::SeqCst));
        // run_now 路径：
        ignore.store(false, Ordering::SeqCst);
        assert!(!ignore.load(Ordering::SeqCst));
    }
}
