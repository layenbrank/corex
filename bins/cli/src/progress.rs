//! 进度上报口在 CLI 侧的三个实现，以及挑哪个的那个决定。
//!
//! - [`Live`]：给人看的。覆盖式刷新的 spinner，只在 stderr 是终端时用。
//! - [`Lines`]：给人看的。每步一行结论，管道 / 重定向时用。
//! - [`Events`]：给机器看的。stdout 上的 NDJSON。
//!
//! 三者只有 [`steps`] 一个入口，形态各自独立实现，方法里没有「这次要不要画」的分支。
//! 它们都写 stderr（`Events` 除外，它的本职就是 stdout 上的事件流）——stdout 的结果
//! 通道不能被进度污染，`println!` 那条路子早在 `output` 模块就已经堵死了。
//!
//! `--remote` 时这三个实现并不知情：daemon 推回来的帧经 `corex_ipc::Replay`
//! 重放进同一个上报口，于是本地与远程两条路径共用这一套渲染。

use crate::output::{self, error_line, line};
use corex_core::{Mark, Observer, Spot, Unit};
use corex_ipc::ProgressEvent;
use corex_updater::human_size;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// 人类可读的步骤进度：终端用覆盖式刷新，管道用每步一行，`--quiet` 完全不上报。
///
/// 三种形态在这里一次性选定，之后各走各的 [`Observer`] 实现——上游方法里因此不再有
/// 「这次要不要画」这类分支。`None` 不是「什么都不做」，而是**不挂上报口**：
/// 引擎那边于是连 `file.copy` 的分块拷贝都不会启用。
pub(crate) fn human(quiet: bool) -> Option<Arc<dyn Observer>> {
    match (quiet, std::io::stderr().is_terminal()) {
        (true, _) => None,
        (false, true) => Some(Arc::new(Live::start())),
        (false, false) => Some(Arc::new(Lines)),
    }
}

/// 覆盖式刷新的进度条：一块多行画布，每个进行中的步骤一条。
pub(crate) struct Live {
    canvas: MultiProgress,
    /// 按「动作 + 步骤」定位。`parallel` 步骤下会同时存在多条。
    running: Mutex<HashMap<String, ProgressBar>>,
}

impl Live {
    fn start() -> Self {
        Self {
            canvas: MultiProgress::with_draw_target(ProgressDrawTarget::stderr()),
            running: Mutex::new(HashMap::new()),
        }
    }

    /// 上报口要在 `parallel` 分支的并发任务里被调用，所以用互斥量而不是 `RefCell`。
    /// 锁中毒只会发生在别的线程 panic 之后，那时继续画进度无害，故不 `unwrap`。
    fn running(&self) -> MutexGuard<'_, HashMap<String, ProgressBar>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// 手写而不是派生：`ProgressBar` / `Mutex` 的内部状态对调试这个上报口没有帮助，
/// 而 `Observer` 要求 `Debug` 只是为了让 `ExecutionContext` 能派生它。
impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Live").finish_non_exhaustive()
    }
}

impl Observer for Live {
    fn begin(&self, at: Spot<'_>) {
        let bar = self.canvas.add(ProgressBar::new_spinner());
        bar.set_style(spinner());
        bar.set_message(title(at));
        bar.enable_steady_tick(Duration::from_millis(120));
        self.running().insert(key(at), bar);
    }

    fn chunk(&self, at: Spot<'_>, mark: Mark) {
        let looked_up = self.running().get(&key(at)).cloned();
        if let Some(bar) = looked_up {
            bar.set_message(progress(at, mark));
        }
    }

    fn end(&self, at: Spot<'_>, took: Duration, ok: bool) {
        if let Some(bar) = self.running().remove(&key(at)) {
            bar.finish_and_clear();
            self.canvas.remove(&bar);
        }
        // 结论行得在画布让开的时候写：并行分支里兄弟步骤的 spinner 还在刷新，
        // 直接往 stderr 写，会被下一次重绘连同它前面那几行一起清掉。
        let line = conclusion(at, took, ok);
        self.canvas.suspend(|| error_line(&line));
    }
}

/// 非终端下的进度：每步只留一行结论。
///
/// 进度条那种覆盖式刷新在日志里只会变成噪声，而结论行滚出去之后恰好就是一份
/// 可读的执行记录——这正是重定向到文件时想要的东西。
#[derive(Debug)]
pub(crate) struct Lines;

impl Observer for Lines {
    fn end(&self, at: Spot<'_>, took: Duration, ok: bool) {
        error_line(&conclusion(at, took, ok));
    }
}

/// 步骤在进度表里的键：动作与步骤 id 一起才能区分并行分支里的同名步骤。
fn key(at: Spot<'_>) -> String {
    format!("{}\u{1}{}", at.action, at.id)
}

/// 标题：`file.copy  copy`。
fn title(at: Spot<'_>) -> String {
    format!("{}  {}", at.action, at.id)
}

/// 标题带上数量：`file.copy  copy  12.4 MB / 88.1 MB (14%)`。
fn progress(at: Spot<'_>, mark: Mark) -> String {
    format!("{}  {}", title(at), amounts(mark))
}

/// 数量部分：`12.4 MB / 88.1 MB (14%)`。
fn amounts(mark: Mark) -> String {
    let done = quantity(mark.unit, mark.done);
    match mark.total {
        Some(total) if total > 0 => format!(
            "{done} / {} ({}%)",
            quantity(mark.unit, total),
            percent(mark)
        ),
        Some(total) => format!("{done} / {}", quantity(mark.unit, total)),
        None => done,
    }
}

fn quantity(unit: Unit, n: u64) -> String {
    match unit {
        Unit::Bytes => human_size(n),
        Unit::Items => format!("{n} 项"),
    }
}

fn percent(mark: Mark) -> u64 {
    match mark.total {
        Some(total) if total > 0 => mark.done.min(total).saturating_mul(100) / total,
        _ => 0,
    }
}

/// 结束行：`✓ file.copy  copy  12ms`。符号与颜色都取自 `output` 的两张表。
fn conclusion(at: Spot<'_>, took: Duration, ok: bool) -> String {
    let (role, glyph) = if ok {
        (output::Role::Ok, output::symbols().ok)
    } else {
        (output::Role::Bad, output::symbols().bad)
    };
    format!(
        "{} {}  {}",
        output::paint_err(role, glyph),
        title(at),
        elapsed(took)
    )
}

/// 墙钟耗时的紧凑写法：不足一秒用毫秒，否则保留两位小数。
fn elapsed(took: Duration) -> String {
    if took.as_millis() < 1000 {
        format!("{}ms", took.as_millis())
    } else {
        format!("{:.2}s", took.as_secs_f64())
    }
}

fn spinner() -> ProgressStyle {
    // 模板是编译期常量，写错了是编码错误而不是运行期状况；帧来自符号表。
    ProgressStyle::with_template("{spinner} {msg}")
        .expect("进度模板是编译期常量")
        .tick_chars(output::symbols().tick)
}

/// 给机器看的步骤事件：stdout 上的 NDJSON。
///
/// 每一行就是 [`ProgressEvent`] 的 JSON——与 daemon 在这条指令上推回来的帧**同一种词汇**，
/// 所以宿主不必为「本地跑的」与「远程跑的」记两套字段名。整条指令跑完（或失败）后
/// 再追加一条 `result` / `error`，它们只是多了一个 `kind`，不属于进度本身。
///
/// `seq` 只在 `step_start` 上递增，宿主据此给步骤编号；`step_progress` / `step_end`
/// 靠 `step` + `action` 归属。
pub(crate) struct Events {
    seq: AtomicU64,
}

impl std::fmt::Debug for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Events").finish()
    }
}

impl Events {
    pub(crate) fn new() -> Self {
        Self {
            seq: AtomicU64::new(0),
        }
    }

    /// 事件流与最终结果走同一条 stdout，调用方按「一行一个 JSON」来读。
    ///
    /// 序列化失败只记 debug：它不该把一条已经跑完的指令降级成失败。
    pub(crate) fn emit<T: serde::Serialize>(&self, payload: &T) {
        match serde_json::to_string(payload) {
            Ok(encoded) => line(&encoded),
            Err(err) => tracing::debug!(error = %err, "事件序列化失败"),
        }
    }
}

impl Observer for Events {
    fn begin(&self, at: Spot<'_>) {
        self.emit(&ProgressEvent::StepStart {
            seq: self.seq.fetch_add(1, Ordering::Relaxed) + 1,
            step: at.id.to_string(),
            action: at.action.to_string(),
        });
    }

    fn chunk(&self, at: Spot<'_>, mark: Mark) {
        self.emit(&ProgressEvent::StepProgress {
            step: at.id.to_string(),
            action: at.action.to_string(),
            done: mark.done,
            total: mark.total,
            unit: mark.unit,
        });
    }

    fn end(&self, at: Spot<'_>, took: Duration, ok: bool) {
        self.emit(&ProgressEvent::StepEnd {
            step: at.id.to_string(),
            action: at.action.to_string(),
            took_ms: took.as_millis() as u64,
            ok,
        });
    }
}
