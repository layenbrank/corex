//! watch 流水线的两级计时门：**防抖**（debounce）与**节流**（throttle）。
//!
//! ```text
//! FS 事件 ──合并同一文件的连续事件──► 防抖门(debounce_ms)──► 节流门(throttle_ms)──► run_directive_file
//! ```
//!
//! 两级是**同一台状态机**（[`Gate`]），差别只有 `max_wait` —— 这正是 lodash 的定义：
//!
//! ```js
//! _.throttle(fn, w, o)  ===  _.debounce(fn, w, { ...o, maxWait: w })
//! ```
//!
//! | 级   | `max_wait`     | 连续触发时的保证                                     |
//! | ---- | -------------- | ---------------------------------------------------- |
//! | 防抖 | `None`         | 一阵抖动**只跑一次**：安静 `wait` 之后才跑（`trailing` 时） |
//! | 节流 | `Some(wait)`   | **每 `wait` 至少跑一次**（`maxWait` 到期强制执行）       |
//!
//! [`Edge`]（`leading` / `trailing`）只决定「哪条边沿执行」，两级通用；两级的差别
//! 来自 `max_wait`，**不是**来自边沿。边沿判定**与流水线忙不忙无关**。
//!
//! 与 lodash 的差别只有一点：这里是**单飞**流水线（CAS + `is_running`），
//! 放行之后 worker 会先等当前那一轮收尾再执行 —— 只会**推迟**，不会像 lodash
//! 那样并发调用；若那次执行被 `RUN_NOW` 抢走 CAS，由 `retry` 补上。

use crate::trigger::{Edge, WatchConfig};
use std::time::{Duration, Instant};

/// lodash `debounce` / `throttle` 的状态机（见模块文档）。
///
/// 入口只有两个：`note`（触发到达）与 `take_due`（定时到期），都只回答
/// 「这一轮放行吗」；排期由 `pending` 给出。
#[derive(Debug)]
struct Gate {
    wait: Duration,
    /// `None` = 防抖；`Some(wait)` = 节流（lodash 的 `maxWait`）。
    max_wait: Option<Duration>,
    /// 执行时机 = lodash 的 `leading` / `trailing` 选项。
    edge: Edge,
    /// 最后一次触发（lodash `lastCallTime`）。
    last_call: Option<Instant>,
    /// 最后一次执行开始（lodash `lastInvokeTime`）。
    last_run: Option<Instant>,
    /// 定时器到期时刻（lodash `timerId`）；`None` = 没有定时器。
    pending: Option<Instant>,
    /// 自上次执行以来是否还有没被观察到的触发（lodash `lastArgs`）。
    has_args: bool,
    /// 下一次定时到点**强制执行**，不再判定边沿。
    ///
    /// 只在「已经通过边沿判定、却没能执行」时置位：被 `RUN_NOW` 抢走 CAS，
    /// 或刚跑完一轮却发现运行期间还积着触发。
    force: bool,
}

impl Gate {
    fn debounce(wait: Duration, edge: Edge) -> Self {
        Self::new(wait, None, edge)
    }

    fn throttle(wait: Duration, edge: Edge) -> Self {
        Self::new(wait, Some(wait), edge)
    }

    fn new(wait: Duration, max_wait: Option<Duration>, edge: Edge) -> Self {
        Self {
            wait,
            max_wait,
            edge,
            last_call: None,
            last_run: None,
            pending: None,
            has_args: false,
            force: false,
        }
    }

    /// 触发到达（lodash 的 `debounced()`）；`true` = 放行。
    ///
    /// 这里**不管**流水线是否正在执行：单飞交给 worker（放行之后它会先等当前
    /// 那轮收尾），所以门只按边沿如实判定。
    fn note(&mut self, now: Instant) -> bool {
        let invoke = self.can_invoke(now);
        self.last_call = Some(now);
        self.has_args = true;

        if invoke && self.pending.is_none() {
            return self.try_leading_edge(now);
        }
        if invoke && self.max_wait.is_some() {
            // lodash 的 maxWait 强制边沿：连续触发下按 wait 稳定执行。
            self.last_run = Some(now);
            self.pending = Some(now + self.wait);
            self.has_args = false;
            return true;
        }
        if self.pending.is_none() {
            // 窗口内、且还没有定时器：排到窗口结束再看。
            self.pending = Some(now + self.wait);
        }
        false
    }

    /// 定时到期（lodash 的 `timerExpired` + `trailingEdge`）。
    fn take_due(&mut self, now: Instant) -> bool {
        if self.pending.is_none_or(|until| until > now) {
            return false;
        }
        if self.force {
            self.force = false;
            self.pending = None;
            self.has_args = false;
            self.last_run = Some(now);
            return true;
        }
        if !self.can_invoke(now) {
            // 还没到：重新排期（`remaining_wait` 会跟着新的触发往后推）。
            self.pending = Some(now + self.remaining_wait(now));
            return false;
        }
        self.pending = None;
        // lodash 的 `trailingEdge`：只有 trailing 边沿且攒着触发才执行。
        if !self.edge.is_trailing() || !self.has_args {
            self.has_args = false;
            return false;
        }
        self.has_args = false;
        self.last_run = Some(now);
        true
    }

    /// 首次触发边沿（lodash 的 `leadingEdge`）；`true` = 放行。
    fn try_leading_edge(&mut self, now: Instant) -> bool {
        self.last_run = Some(now);
        self.pending = Some(now + self.wait);
        if !self.edge.is_leading() {
            return false;
        }
        self.has_args = false;
        true
    }

    /// 外部调用（`RUN_NOW` / `immediate`）：按 lodash 的 `invokeFunc` 记一次执行。
    ///
    /// **不动** `has_args`：已经攒下的触发照样会在窗口结束后补跑。
    fn note_external(&mut self, at: Instant) {
        self.last_call = Some(at);
        self.last_run = Some(at);
    }

    /// 已经通过边沿判定、却没能执行的那次触发：挪到最近一轮强制执行。
    fn retry(&mut self, now: Instant) {
        self.force = true;
        self.pending = Some(now);
    }

    /// 定时器到期时刻。
    fn pending(&self) -> Option<Instant> {
        self.pending
    }

    /// lodash 的 `shouldInvoke`：现在允许执行吗。
    fn can_invoke(&self, now: Instant) -> bool {
        let Some(call) = self.last_call else {
            return true;
        };
        if now.saturating_duration_since(call) >= self.wait {
            return true;
        }
        match (self.max_wait, self.last_run) {
            (Some(max), Some(run)) => now.saturating_duration_since(run) >= max,
            _ => false,
        }
    }

    /// lodash 的 `remainingWait`：还要等多久才允许执行。
    fn remaining_wait(&self, now: Instant) -> Duration {
        let wait_left = match self.last_call {
            Some(call) => self
                .wait
                .saturating_sub(now.saturating_duration_since(call)),
            None => self.wait,
        };
        match (self.max_wait, self.last_run) {
            (Some(max), Some(run)) => {
                wait_left.min(max.saturating_sub(now.saturating_duration_since(run)))
            }
            _ => wait_left,
        }
    }
}

/// 触发链：防抖门 → 节流门。两级都放行才会执行。
#[derive(Debug)]
pub(super) struct Chain {
    debounce: Gate,
    throttle: Gate,
}

impl Chain {
    pub(super) fn new(cfg: &WatchConfig) -> Self {
        Self {
            debounce: Gate::debounce(Duration::from_millis(cfg.debounce_ms), cfg.debounce),
            throttle: Gate::throttle(Duration::from_millis(cfg.throttle_ms), cfg.throttle),
        }
    }

    /// 新触发到达；`true` = 现在就该执行（两级都放行）。
    pub(super) fn note(&mut self, now: Instant) -> bool {
        self.debounce.note(now) && self.throttle.note(now)
    }

    /// 定时到期；`true` = 现在就该执行（两级都放行）。
    pub(super) fn advance(&mut self, now: Instant) -> bool {
        if self.debounce.take_due(now) && self.throttle.note(now) {
            return true;
        }
        self.throttle.take_due(now)
    }

    /// 待执行时刻：两级取早。
    pub(super) fn pending(&self) -> Option<Instant> {
        match (self.debounce.pending(), self.throttle.pending()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// 外部调用（`RUN_NOW` / `immediate`）。
    pub(super) fn note_external(&mut self, at: Instant) {
        self.debounce.note_external(at);
        self.throttle.note_external(at);
    }

    /// 已经通过边沿判定、却没能执行的那次触发（CAS 被抢走 / 刚跑完一轮）。
    pub(super) fn retry(&mut self, now: Instant) {
        self.throttle.retry(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// 两种边沿的一份配置（两个窗口都取 100ms 的整数倍，便于断言）。
    fn chain(debounce: Edge, throttle: Edge, debounce_ms: u64, throttle_ms: u64) -> Chain {
        Chain::new(&WatchConfig {
            paths: vec![".".into()],
            includes: Vec::new(),
            excludes: Vec::new(),
            debounce_ms,
            debounce,
            throttle_ms,
            throttle,
            immediate: false,
            poll: false,
            events: Vec::new(),
        })
    }

    /// 把定时器推进到 `now`（镜像 worker：到点就 `take_due`）；返回执行次数。
    fn timer(g: &mut Gate, now: Instant) -> u32 {
        let mut runs = 0;
        while g.pending().is_some_and(|until| until <= now) {
            if g.take_due(now) {
                runs += 1;
            }
        }
        runs
    }

    /// 按 lodash 的方式喂一串触发：每次触发前先把定时器推到当前时刻。
    fn burst(g: &mut Gate, from: Instant, gap_ms: u64, times: u32) -> u32 {
        let mut runs = 0;
        for step in 1..=times {
            let at = from + ms(gap_ms * u64::from(step));
            runs += timer(g, at);
            if g.note(at) {
                runs += 1;
            }
        }
        runs
    }

    // ---- 防抖：没有 maxWait，一阵抖动只跑一次 ----

    #[test]
    fn debounce_trailing_runs_once_after_quiet() {
        let mut g = Gate::debounce(ms(100), Edge::TRAILING);
        let t0 = Instant::now();
        assert!(!g.note(t0), "trailing 不立即跑");
        assert_eq!(g.pending(), Some(t0 + ms(100)));
        assert!(!g.note(t0 + ms(30)));
        assert!(!g.note(t0 + ms(60)));
        // 定时器到点才发现安静期被推后了：重新排到「最后一个触发 + wait」。
        assert!(!g.take_due(t0 + ms(100)));
        assert_eq!(g.pending(), Some(t0 + ms(160)));
        assert!(!g.take_due(t0 + ms(159)));
        assert!(g.take_due(t0 + ms(160)));
        assert!(!g.take_due(t0 + ms(200)), "只跑一次");
    }

    /// lodash 的关键行为：`leading` 防抖在连续触发下**只跑一次**
    /// （走的是「安静期」而不是「最小间隔」）。
    #[test]
    fn debounce_leading_runs_once_per_burst() {
        let mut g = Gate::debounce(ms(100), Edge::LEADING);
        let t0 = Instant::now();
        assert!(g.note(t0));
        assert_eq!(burst(&mut g, t0, 50, 5), 0, "安静期内不会重复 leading");
        // 停止触发、安静期过去之后，下一个触发重新走 leading。
        let after = t0 + ms(500);
        timer(&mut g, after + ms(100));
        assert!(g.note(after + ms(100)));
    }

    #[test]
    fn debounce_both_leading_then_one_trailing() {
        let mut g = Gate::debounce(ms(100), Edge::BOTH);
        let t0 = Instant::now();
        assert!(g.note(t0));
        assert!(!g.note(t0 + ms(20)));
        assert!(!g.note(t0 + ms(40)));
        // 安静期结束补跑一次，且只有一次。
        assert_eq!(timer(&mut g, t0 + ms(500)), 1);
    }

    #[test]
    fn debounce_after_quiet_never_trails_without_new_triggers() {
        let mut g = Gate::debounce(ms(100), Edge::BOTH);
        let t0 = Instant::now();
        assert!(g.note(t0));
        // 之后没有新触发 → 不补跑。
        assert_eq!(timer(&mut g, t0 + ms(500)), 0);
    }

    // ---- 节流：maxWait = wait，连续触发下每 wait 至少跑一次 ----

    #[test]
    fn throttle_leading_keeps_running_while_triggers_keep_coming() {
        let mut g = Gate::throttle(ms(100), Edge::LEADING);
        let t0 = Instant::now();
        assert!(g.note(t0));
        // 每 50ms 一个触发：maxWait 到期时强制执行（lodash 的 maxing 分支）。
        assert_eq!(burst(&mut g, t0, 50, 6), 3);
    }

    #[test]
    fn throttle_trailing_delays_the_first_run_by_wait() {
        let mut g = Gate::throttle(ms(100), Edge::TRAILING);
        let t0 = Instant::now();
        assert!(!g.note(t0), "trailing 不立即跑");
        assert_eq!(g.pending(), Some(t0 + ms(100)));
        // 窗口结束时跑，之后按 wait 稳定执行。
        assert_eq!(timer(&mut g, t0 + ms(100)), 1);
        assert!(!g.note(t0 + ms(110)));
        assert_eq!(g.pending(), Some(t0 + ms(210)));
        assert_eq!(timer(&mut g, t0 + ms(210)), 1);
    }

    #[test]
    fn throttle_both_is_leading_plus_one_trailing_per_window() {
        let mut g = Gate::throttle(ms(100), Edge::BOTH);
        let t0 = Instant::now();
        assert!(g.note(t0));
        assert!(!g.note(t0 + ms(10)));
        assert_eq!(g.pending(), Some(t0 + ms(100)));
        assert_eq!(timer(&mut g, t0 + ms(100)), 1);
        // 窗口结束后新的触发重新走 leading。
        assert!(g.note(t0 + ms(150)));
    }

    /// 边沿判定只看窗口，不看流水线忙不忙：`leading` 窗口内一律不补跑。
    #[test]
    fn leading_drops_in_window_triggers_without_a_rerun() {
        let mut g = Gate::throttle(ms(100), Edge::LEADING);
        let t0 = Instant::now();
        assert!(g.note(t0));
        // 窗口内：既不执行，也不排期。
        assert!(!g.note(t0 + ms(10)));
        assert_eq!(g.pending(), Some(t0 + ms(100)));
        assert_eq!(timer(&mut g, t0 + ms(100)), 0, "窗口内不补跑");
        assert!(g.note(t0 + ms(101)), "窗口过后重新 leading");
    }

    /// `trailing` 不丢：窗口内攒下的触发一定会有一次执行。
    #[test]
    fn trailing_never_drops_in_window_triggers() {
        let mut g = Gate::throttle(ms(100), Edge::TRAILING);
        let t0 = Instant::now();
        assert!(!g.note(t0));
        assert!(!g.note(t0 + ms(10)));
        assert_eq!(timer(&mut g, t0 + ms(500)), 1);
    }

    // ---- 已经通过边沿判定、却没能执行的那次触发 ----

    #[test]
    fn retry_runs_as_soon_as_possible() {
        let mut g = Gate::throttle(ms(100), Edge::BOTH);
        let t0 = Instant::now();
        assert!(g.note(t0));
        g.retry(t0 + ms(10));
        assert!(g.take_due(t0 + ms(10)), "CAS 失败不等窗口");
    }

    /// `RUN_NOW` 记一次执行：紧随其后的触发不会又 leading 一次，
    /// 已排好的待执行会跟着窗口一起顺延。
    #[test]
    fn external_invoke_moves_the_window() {
        let mut g = Gate::throttle(ms(100), Edge::BOTH);
        let t0 = Instant::now();
        assert!(g.note(t0));
        assert!(!g.note(t0 + ms(20)));

        g.note_external(t0 + ms(50));
        assert!(!g.take_due(t0 + ms(100)), "窗口被 RUN_NOW 推后了");
        assert_eq!(timer(&mut g, t0 + ms(1000)), 1, "补跑仍然会发生");
    }

    // ---- 两级串起来 ----

    /// 默认边沿（防抖 trailing + 节流 both）：一批抖动跑一次，
    /// 上一次执行之后才发生的改动会被合并成一次补跑，不会丢掉。
    #[test]
    fn default_edges_coalesce_a_burst_into_one_run() {
        let mut c = chain(Edge::TRAILING, Edge::BOTH, 100, 1000);
        let t0 = Instant::now();
        assert!(!c.note(t0));
        assert!(!c.note(t0 + ms(50)));
        assert_eq!(c.pending(), Some(t0 + ms(100)));

        // 安静期还没到：定时器只是重新排期。
        assert!(!c.advance(t0 + ms(100)));
        assert_eq!(c.pending(), Some(t0 + ms(150)));
        // 安静期结束，且节流门在窗口外 → 立即执行。
        assert!(c.advance(t0 + ms(150)));

        // 上一次执行之后到达的改动 → 节流窗口结束时补跑一次。
        assert!(!c.note(t0 + ms(200)));
        assert!(!c.advance(t0 + ms(300)), "节流窗口内先攒着");
        assert!(c.advance(t0 + ms(1150)));
    }

    /// 防抖 leading + 节流 leading：首个事件立即跑，其余在窗口内被忽略。
    #[test]
    fn leading_edges_run_the_first_trigger_only() {
        let mut c = chain(Edge::LEADING, Edge::LEADING, 100, 1000);
        let t0 = Instant::now();
        assert!(c.note(t0));
        assert!(!c.note(t0 + ms(50)));
        assert!(!c.advance(t0 + ms(150)));
        assert!(!c.advance(t0 + ms(500)));
    }
}
