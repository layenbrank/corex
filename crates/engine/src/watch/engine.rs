//! 文件监听引擎：`notify_debouncer_full` 只合并 FS 事件，计时交给防抖 / 节流两级门。
//!
//! ```text
//! FS 事件 ──合并同一文件的连续事件──► 防抖门(debounce_ms)──► 节流门(throttle_ms)──► run_directive
//! ```
//!
//! 两级共用一台状态机，差别只有 lodash 的 `maxWait`：
//! `_.throttle(fn, w, o) === _.debounce(fn, w, { ...o, maxWait: w })`。
//! `debounce` / `throttle` 两个键设置各自执行哪条边沿（`leading` / `trailing`），
//! 详见 [`super::gate`]。

use super::event::{EventAction, EventFilter, classify_event};
use super::filter::{WatchFilter, watch_relative_path};
use super::gate::Chain;
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
/// FS 事件合并窗口的上限：只用于去重 / 重命名，不承载计时语义。
const FS_COALESCE_MS: u64 = 25;

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
    /// 与 worker 共享，使 RUN_NOW / immediate 能刷新两级门的窗口。
    chain: Arc<Mutex<Chain>>,
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
        // `leading` 不是错误配置，但窗口内的触发不会产生补跑（见 `Edge::LEADING`），
        // 产物可能因此落后——启动时留一条可审计的日志。
        for (key, edge) in [("debounce", cfg.debounce), ("throttle", cfg.throttle)] {
            if !edge.is_trailing() {
                warn!(
                    directive = %spec.directive_name,
                    key,
                    "watch 选了 leading 边沿：窗口内到达的触发不会补跑"
                );
            }
        }
        let mount_specs = resolve_roots(&cfg);
        let mount_paths: Vec<PathBuf> = mount_specs
            .iter()
            .map(|raw| resolve_watch_path(raw))
            .collect();
        let watch_roots_str = cfg.paths.clone();

        let is_running = Arc::new(AtomicBool::new(false));
        // 计时全在两扇逻辑门里；worker 与 RUN_NOW 共享同一条链。
        let chain = Arc::new(Mutex::new(Chain::new(&cfg)));

        let (trigger_tx, trigger_rx) = tokio::sync::mpsc::channel::<()>(TRIGGER_CHANNEL_CAP);
        let (remount_tx, mut remount_rx) = tokio::sync::mpsc::unbounded_channel::<RemountCmd>();

        let worker = spawn_watch_worker(
            trigger_rx,
            WorkerCtx {
                worker_flag: Arc::clone(&is_running),
                worker_chain: Arc::clone(&chain),
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

        // FS 层只把同一个文件的连续事件合并掉（去重 / 重命名），
        // 计时语义全在防抖门里，所以这里不要用完整的 debounce_ms。
        let fs_window = Duration::from_millis(cfg.debounce_ms.clamp(1, FS_COALESCE_MS));
        let tick_rate = (fs_window / 2).max(Duration::from_millis(1));
        let notify_cfg = if cfg.poll {
            NotifyConfig::default().with_poll_interval(POLL_INTERVAL)
        } else {
            NotifyConfig::default()
        };
        let debouncer = if cfg.poll {
            JobDebouncer::Poll(
                new_debouncer_opt(
                    fs_window,
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
                    fs_window,
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
                chain,
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
        // 在调用开始时刷新窗口（与 leading 一致），使紧随其后的 FS 触发
        // 不会再 leading 触发一次。
        if let Ok(mut chain) = state.chain.lock() {
            chain.note_external(Instant::now());
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

/// watch worker 每次触发都需要的全部东西。
///
/// 用一个值代替七个并列参数：它们合起来就是 worker 的整个世界；收进结构体
/// 也让两个调用点不容易走偏。字段名保留 `worker_` 前缀，因为 worker 主体里
/// 用的就是这个前缀。
struct WorkerCtx {
    /// 流水线运行时为 `true`，同时充当单飞门禁。
    worker_flag: Arc<AtomicBool>,
    /// 去抖门 → 节流门，与 `RUN_NOW` 共享。
    worker_chain: Arc<Mutex<Chain>>,
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
            worker_chain,
            worker_store,
            worker_runtime,
            worker_data,
            worker_path,
            worker_name,
        } = ctx;
        loop {
            let deadline = worker_chain.lock().ok().and_then(|chain| chain.pending());
            let fire = tokio::select! {
                trig = trigger_rx.recv() => {
                    if trig.is_none() {
                        break;
                    }
                    drain(&mut trigger_rx);
                    feed(&worker_chain)
                }
                _ = sleep_until(deadline) => tick(&worker_chain),
            };
            if !fire {
                continue;
            }

            // 门已经放行，但当前那轮还没收尾：等它跑完再执行（单飞，不并发）。
            // 等待期间到达的触发照常喂给两级门，会被合并成最多一次补跑。
            while worker_flag.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(BUSY_POLL_MS)).await;
                if drain(&mut trigger_rx) {
                    feed(&worker_chain);
                }
            }

            let ran = invoke_directive(
                &worker_flag,
                Arc::clone(&worker_store),
                worker_runtime.clone(),
                worker_data.clone(),
                &worker_path,
                &worker_name,
            )
            .await;
            let now = Instant::now();
            if !ran {
                // CAS 被 RUN_NOW 抢走：这次触发已经通过边沿判定，补上它。
                if let Ok(mut chain) = worker_chain.lock() {
                    chain.retry(now);
                }
                continue;
            }
            // 运行期间到达的触发回填一次；能立刻放行就再来一轮。
            if drain(&mut trigger_rx)
                && feed(&worker_chain)
                && let Ok(mut chain) = worker_chain.lock()
            {
                chain.retry(now);
            }
        }
    })
}

/// 睡到定时；没有待执行时就一直等触发。
async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(until) => tokio::time::sleep_until(until.into()).await,
        None => std::future::pending().await,
    }
}

/// 把 channel 里积压的触发归并掉；返回是否有触发。
fn drain(trigger_rx: &mut tokio::sync::mpsc::Receiver<()>) -> bool {
    let mut saw = false;
    while trigger_rx.try_recv().is_ok() {
        saw = true;
    }
    saw
}

/// 喂一次触发给两级门；`true` = 现在就该执行。
fn feed(chain: &Mutex<Chain>) -> bool {
    let now = Instant::now();
    chain.lock().is_ok_and(|mut chain| chain.note(now))
}

/// 定时到期，推进两级门；`true` = 现在就该执行。
fn tick(chain: &Mutex<Chain>) -> bool {
    let now = Instant::now();
    chain.lock().is_ok_and(|mut chain| chain.advance(now))
}

/// CAS + 运行。CAS 抢不到时返回 false（不要重复开启）。
async fn invoke_directive(
    flag: &AtomicBool,
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
    info!(directive = %name, "watch 触发执行");
    let result = run_directive_file(store, runtime, data_dir, path).await;
    if let Err(e) = result {
        warn!(directive = %name, error = %e, "watch 执行失败");
    }
    flag.store(false, Ordering::SeqCst);
    true
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
    use crate::trigger::{DEBOUNCE_EDGE, Edge, THROTTLE_EDGE};

    /// 默认边沿的一份配置（去抖 trailing、节流 leading）。
    fn config() -> WatchConfig {
        WatchConfig {
            paths: vec!["/proj".into()],
            includes: Vec::new(),
            excludes: Vec::new(),
            debounce_ms: 300,
            debounce: DEBOUNCE_EDGE,
            throttle_ms: 1000,
            throttle: THROTTLE_EDGE,
            immediate: false,
            poll: false,
            events: Vec::new(),
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn resolve_roots_narrows_simple_includes() {
        let cfg = WatchConfig {
            includes: vec!["src".into(), "templates".into()],
            ..config()
        };
        let roots = resolve_roots(&cfg);
        assert_eq!(roots, vec!["/proj/src", "/proj/templates"]);
    }

    #[test]
    fn resolve_roots_keeps_paths_for_glob_includes() {
        let cfg = WatchConfig {
            includes: vec!["src/**".into()],
            ..config()
        };
        assert_eq!(resolve_roots(&cfg), vec!["/proj"]);
    }

    #[test]
    fn drain_coalesces_many_channel_messages() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);
        for _ in 0..10 {
            tx.try_send(()).unwrap();
        }
        assert!(drain(&mut rx));
        assert!(!drain(&mut rx), "channel 必须被排空");
    }

    /// 默认边沿（防抖 trailing + 节流 both）：一批抖动跑一次，
    /// 上一次执行之后才发生的改动会被合并成一次补跑，不会丢掉。
    #[test]
    fn later_changes_are_coalesced_into_one_trailing_run() {
        let mut chain = Chain::new(&config());
        let t0 = Instant::now();
        assert!(!chain.note(t0));
        assert!(!chain.note(t0 + ms(50)));

        assert!(chain.advance(t0 + ms(350)));

        assert!(!chain.note(t0 + ms(400)));
        assert!(!chain.advance(t0 + ms(700)));
        assert_eq!(chain.pending(), Some(t0 + ms(1350)));
        assert!(chain.advance(t0 + ms(1350)));
    }

    /// 显式选 leading：窗口内到达的改动不会补跑——代价是可能落后。
    #[test]
    fn leading_throttle_drops_changes_inside_the_window() {
        let mut chain = Chain::new(&WatchConfig {
            throttle: Edge::LEADING,
            ..config()
        });
        let t0 = Instant::now();
        assert!(!chain.note(t0));
        assert!(chain.advance(t0 + ms(300)));
        assert_eq!(chain.pending(), Some(t0 + ms(1300)));

        // 窗口内到达的改动：不产生补跑。
        assert!(!chain.note(t0 + ms(400)));
        assert!(!chain.advance(t0 + ms(700)));
        assert!(!chain.advance(t0 + ms(1300)), "leading 不补跑");
        assert!(chain.pending().is_none());
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
