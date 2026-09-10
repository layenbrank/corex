//! 面向 watch 流水线的类 lodash 调用节流（`leading: true`、`trailing: true`）。
//!
//! 流水线：FS 事件 → **notify_debouncer_full**（文件系统静默期去抖，不是 lodash
//! debounce 的移植）→ 逻辑触发 → **本节流器** → `run_directive_file`。
//!
//! 计时基准：节流窗口从 **调用开始**（调用时刻）起算，与 lodash throttle
//! 常见的“调用时刻”语义一致。YAML 字段：`throttle_ms`。

use std::time::{Duration, Instant};

/// 逻辑触发（去抖之后）到达时的决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerDecision {
    /// 在节流窗口之外：立即调用（leading 边沿）。
    RunLeading,
    /// 在窗口之内：布置 trailing；在 `until`（上次调用开始 + 间隔）醒来。
    ArmTrailing { until: Instant },
}

/// 对流水线调用限流：每个窗口最多一次 leading、一次 trailing。
#[derive(Debug)]
pub struct InvokeThrottle {
    interval: Duration,
    last_invoke_start: Option<Instant>,
    trailing_pending: bool,
}

impl InvokeThrottle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_invoke_start: None,
            trailing_pending: false,
        }
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn has_trailing(&self) -> bool {
        self.trailing_pending
    }

    pub fn last_invoke_start(&self) -> Option<Instant> {
        self.last_invoke_start
    }

    pub fn window_end(&self) -> Option<Instant> {
        self.last_invoke_start.map(|t| t + self.interval)
    }

    pub fn is_outside_window(&self, now: Instant) -> bool {
        match self.last_invoke_start {
            None => true,
            Some(start) => now.saturating_duration_since(start) >= self.interval,
        }
    }

    /// 在 `now` 记录一次逻辑触发。
    ///
    /// 当 `is_busy`（另一个调用正持有 `is_running`）时，一律布置 trailing，
    /// 绝不尝试并发 leading——别处的 CAS 仍然保证单飞。
    pub fn note_trigger(&mut self, now: Instant, is_busy: bool) -> TriggerDecision {
        if is_busy {
            self.trailing_pending = true;
            let until = self.window_end().filter(|&end| end > now).unwrap_or(now);
            return TriggerDecision::ArmTrailing { until };
        }
        if self.is_outside_window(now) {
            TriggerDecision::RunLeading
        } else {
            self.trailing_pending = true;
            TriggerDecision::ArmTrailing {
                until: self
                    .window_end()
                    .expect("inside window implies last_invoke set"),
            }
        }
    }

    /// 在一次调用真正开始时调用（leading、trailing 或 RUN_NOW）。
    ///
    /// 清掉 trailing：当前调用吸收了待处理的那次边沿。本次运行期间的新触发
    /// 会经由 [`note_trigger`] 重新布置 trailing。
    pub fn mark_invoke_start(&mut self, at: Instant) {
        self.last_invoke_start = Some(at);
        self.trailing_pending = false;
    }

    /// `RUN_NOW` / `immediate`：刷新节流窗口，使下一次 FS 触发
    /// 不会立刻又 leading 触发一次。
    ///
    /// **不**清掉 `trailing_pending`——已经布置过的 FS 触发仍然会
    /// 在新窗口之后得到一次 trailing（窗口已过则尽快）。
    pub fn record_external_invoke(&mut self, at: Instant) {
        self.last_invoke_start = Some(at);
    }

    pub fn arm_trailing(&mut self) {
        self.trailing_pending = true;
    }

    /// 到期（或运行被推迟）时消费 trailing 标志。
    pub fn take_trailing(&mut self) -> bool {
        let had = self.trailing_pending;
        self.trailing_pending = false;
        had
    }
}

/// 睡到 `until`，醒来后再查一次 `window_end`，这样 `RUN_NOW` 可以延长
/// 窗口，而不引发一次多余的提前 trailing 调用。
pub async fn wait_for_trailing_deadline(
    throttle: &std::sync::Mutex<InvokeThrottle>,
    mut until: Instant,
) {
    loop {
        let now = Instant::now();
        if now >= until {
            // 窗口可能已被外部调用挪动；若仍在窗口内就重新排期。
            let refreshed = throttle
                .lock()
                .ok()
                .and_then(|g| g.window_end())
                .filter(|&end| Instant::now() < end);
            if let Some(end) = refreshed {
                until = end;
                continue;
            }
            return;
        }
        tokio::time::sleep(until.saturating_duration_since(now)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn leading_fires_outside_window() {
        let mut t = InvokeThrottle::new(ms(1000));
        let t0 = Instant::now();
        assert_eq!(t.note_trigger(t0, false), TriggerDecision::RunLeading);
        t.mark_invoke_start(t0);
        assert!(!t.has_trailing());
    }

    #[test]
    fn trailing_armed_inside_window_not_extra_without_triggers() {
        let mut t = InvokeThrottle::new(ms(1000));
        let t0 = Instant::now();
        assert_eq!(t.note_trigger(t0, false), TriggerDecision::RunLeading);
        t.mark_invoke_start(t0);
        // 没有后续触发 → 不做 trailing。
        assert!(!t.has_trailing());
        assert!(!t.take_trailing());
    }

    #[test]
    fn multiple_inside_window_arm_single_trailing() {
        let mut t = InvokeThrottle::new(ms(1000));
        let t0 = Instant::now();
        t.mark_invoke_start(t0);
        let mid = t0 + ms(100);
        match t.note_trigger(mid, false) {
            TriggerDecision::ArmTrailing { until } => assert_eq!(until, t0 + ms(1000)),
            other => panic!("expected ArmTrailing, got {other:?}"),
        }
        let mid2 = t0 + ms(200);
        match t.note_trigger(mid2, false) {
            TriggerDecision::ArmTrailing { until } => {
                assert_eq!(until, t0 + ms(1000));
                assert!(t.has_trailing());
            }
            other => panic!("expected ArmTrailing, got {other:?}"),
        }
        assert!(t.take_trailing());
        assert!(!t.has_trailing());
    }

    #[test]
    fn after_window_next_trigger_is_leading_again() {
        let mut t = InvokeThrottle::new(ms(500));
        let t0 = Instant::now();
        t.mark_invoke_start(t0);
        let later = t0 + ms(500);
        assert_eq!(t.note_trigger(later, false), TriggerDecision::RunLeading);
    }

    #[test]
    fn busy_always_arms_trailing_even_outside_window() {
        let mut t = InvokeThrottle::new(ms(1000));
        let t0 = Instant::now();
        // 从未调用过；忙碌中（如 RUN_NOW）→ 尽快 trailing。
        match t.note_trigger(t0, true) {
            TriggerDecision::ArmTrailing { until } => assert_eq!(until, t0),
            other => panic!("expected ArmTrailing, got {other:?}"),
        }
        assert!(t.has_trailing());
    }

    #[test]
    fn record_external_invoke_blocks_immediate_leading() {
        let mut t = InvokeThrottle::new(ms(1000));
        let t0 = Instant::now();
        // RUN_NOW / immediate 会刷新 last_invoke。
        t.record_external_invoke(t0);
        let soon = t0 + ms(10);
        match t.note_trigger(soon, false) {
            TriggerDecision::ArmTrailing { until } => assert_eq!(until, t0 + ms(1000)),
            other => panic!("expected ArmTrailing after external invoke, got {other:?}"),
        }
    }

    #[test]
    fn record_external_keeps_trailing_pending() {
        let mut t = InvokeThrottle::new(ms(1000));
        let t0 = Instant::now();
        t.mark_invoke_start(t0);
        let _ = t.note_trigger(t0 + ms(50), false);
        assert!(t.has_trailing());
        // RUN_NOW 落在窗口中间：保留 trailing，移动窗口。
        let ext = t0 + ms(100);
        t.record_external_invoke(ext);
        assert!(t.has_trailing());
        assert_eq!(t.window_end(), Some(ext + ms(1000)));
    }

    #[test]
    fn mark_invoke_start_clears_trailing() {
        let mut t = InvokeThrottle::new(ms(1000));
        let t0 = Instant::now();
        t.mark_invoke_start(t0);
        let _ = t.note_trigger(t0 + ms(1), false);
        assert!(t.has_trailing());
        t.mark_invoke_start(t0 + ms(1000));
        assert!(!t.has_trailing());
    }

    /// 模拟 worker 的决策：leading + 窗口内连续触发 → 只产生一次 trailing。
    #[test]
    fn simulate_leading_plus_single_trailing_sequence() {
        let mut t = InvokeThrottle::new(ms(100));
        let t0 = Instant::now();
        let mut runs = 0u32;

        assert_eq!(t.note_trigger(t0, false), TriggerDecision::RunLeading);
        t.mark_invoke_start(t0);
        runs += 1;

        for offset in [10u64, 20, 30, 40] {
            match t.note_trigger(t0 + ms(offset), false) {
                TriggerDecision::ArmTrailing { .. } => {}
                other => panic!("expected trailing arm at +{offset}ms, got {other:?}"),
            }
        }
        assert!(t.take_trailing());
        t.mark_invoke_start(t0 + ms(100));
        runs += 1;

        assert_eq!(runs, 2);
        assert!(!t.has_trailing());
        // trailing 之后没有新事件 → 依旧安静。
        assert!(!t.take_trailing());
    }

    #[tokio::test]
    async fn wait_deadline_respects_extended_window() {
        let throttle = std::sync::Mutex::new(InvokeThrottle::new(ms(80)));
        let t0 = Instant::now();
        throttle.lock().unwrap().mark_invoke_start(t0);
        let original_end = t0 + ms(80);

        let throttle_for_ext = &throttle;
        let extender = async {
            tokio::time::sleep(ms(30)).await;
            // 模拟 RUN_NOW 在等待中途刷新 last_invoke。
            throttle_for_ext
                .lock()
                .unwrap()
                .record_external_invoke(Instant::now());
        };

        let waiter = wait_for_trailing_deadline(&throttle, original_end);

        tokio::join!(extender, waiter);
        let end = throttle.lock().unwrap().window_end().unwrap();
        assert!(
            Instant::now() >= end || Instant::now() + ms(5) >= end,
            "waiter should not return long before the extended window"
        );
        // wait 返回后，应当到了（可能被延长的）窗口的边界或之后。
        assert!(throttle.lock().unwrap().is_outside_window(Instant::now()));
    }

    /// 引擎 worker 决策路径的同步替身（不涉及文件系统、不真跑）。
    struct WorkerOrch {
        throttle: InvokeThrottle,
        is_running: bool,
        trailing_deadline: Option<Instant>,
        /// 成功的调用起点（leading、trailing 或外部）。
        invoke_starts: Vec<&'static str>,
        channel: Vec<()>,
    }

    impl WorkerOrch {
        fn new(interval: Duration) -> Self {
            Self {
                throttle: InvokeThrottle::new(interval),
                is_running: false,
                trailing_deadline: None,
                invoke_starts: Vec::new(),
                channel: Vec::new(),
            }
        }

        fn push_trigger(&mut self) {
            self.channel.push(());
        }

        fn drain_channel(&mut self) -> usize {
            let n = self.channel.len();
            self.channel.clear();
            n
        }

        /// 镜像逻辑：recv + coalesce + note_trigger + 可能的 leading。
        fn on_trigger(&mut self, now: Instant) {
            let _ = self.drain_channel();
            let busy = self.is_running;
            let decision = self.throttle.note_trigger(now, busy);
            match decision {
                TriggerDecision::RunLeading => {
                    if !self.cas_start("leading", now) {
                        self.throttle.arm_trailing();
                        self.trailing_deadline = Some(now);
                    }
                }
                TriggerDecision::ArmTrailing { until } => {
                    self.trailing_deadline = Some(until);
                }
            }
        }

        fn cas_start(&mut self, kind: &'static str, now: Instant) -> bool {
            if self.is_running {
                return false;
            }
            self.is_running = true;
            self.throttle.mark_invoke_start(now);
            self.invoke_starts.push(kind);
            true
        }

        /// RUN_NOW / immediate：CAS + record_external（不清 trailing）。
        fn run_now(&mut self, now: Instant) -> bool {
            if self.is_running {
                return false;
            }
            self.is_running = true;
            self.throttle.record_external_invoke(now);
            self.invoke_starts.push("run_now");
            true
        }

        fn finish_run(&mut self, now: Instant) {
            assert!(self.is_running);
            self.is_running = false;
            // trailing_after_run：把 channel 里的残余归并成最多一次 trailing。
            let saw = self.drain_channel() > 0;
            if !saw {
                if self.throttle.has_trailing() {
                    self.trailing_deadline = self
                        .throttle
                        .window_end()
                        .filter(|&e| e > now)
                        .or(Some(now));
                }
                return;
            }
            match self.throttle.note_trigger(now, false) {
                TriggerDecision::RunLeading => {
                    self.throttle.arm_trailing();
                    self.trailing_deadline = Some(now);
                }
                TriggerDecision::ArmTrailing { until } => {
                    self.trailing_deadline = Some(until);
                }
            }
        }

        /// 到期时触发 trailing（在 wait 之后）；会尊重被延长的窗口。
        fn try_trailing(&mut self, now: Instant) -> bool {
            let Some(until) = self.trailing_deadline else {
                return false;
            };
            if now < until {
                return false;
            }
            // 刷新 wait_for_trailing_deadline
            if let Some(end) = self.throttle.window_end().filter(|&e| now < e) {
                self.trailing_deadline = Some(end);
                return false;
            }
            self.trailing_deadline = None;
            if !self.throttle.take_trailing() {
                return false;
            }
            if self.is_running {
                // 忙等路径：把 channel 当作忙碌 trailing 吸收掉。
                let _ = self.drain_channel();
                let _ = self.throttle.note_trigger(now, true);
                return false; // caller must retry after finish
            }
            if !self.throttle.is_outside_window(now)
                && let Some(end) = self.throttle.window_end()
            {
                self.throttle.arm_trailing();
                self.trailing_deadline = Some(end);
                return false;
            }
            if !self.cas_start("trailing", now) {
                self.throttle.arm_trailing();
                self.trailing_deadline = Some(now);
                return false;
            }
            true
        }
    }

    #[test]
    fn orch_leading_plus_burst_single_trailing() {
        let mut w = WorkerOrch::new(ms(100));
        let t0 = Instant::now();

        w.push_trigger();
        w.on_trigger(t0);
        assert_eq!(w.invoke_starts, ["leading"]);
        w.finish_run(t0 + ms(5));

        // 窗口内的连续触发 → 只布置一次 trailing。
        for off in [10u64, 20, 30, 40] {
            w.push_trigger();
            w.on_trigger(t0 + ms(off));
        }
        assert!(w.throttle.has_trailing());
        assert_eq!(w.trailing_deadline, Some(t0 + ms(100)));
        assert_eq!(w.invoke_starts.len(), 1);

        assert!(w.try_trailing(t0 + ms(100)));
        assert_eq!(w.invoke_starts, ["leading", "trailing"]);
        w.finish_run(t0 + ms(105));
        assert!(!w.throttle.has_trailing());
        assert!(w.trailing_deadline.is_none() || !w.throttle.has_trailing());
    }

    #[test]
    fn orch_run_now_extends_trailing_window() {
        let mut w = WorkerOrch::new(ms(100));
        let t0 = Instant::now();

        w.push_trigger();
        w.on_trigger(t0);
        w.finish_run(t0 + ms(1));

        w.push_trigger();
        w.on_trigger(t0 + ms(20));
        assert_eq!(w.trailing_deadline, Some(t0 + ms(100)));

        // 等待中途的 RUN_NOW 会延长窗口；trailing 不能在旧期限触发。
        assert!(w.run_now(t0 + ms(50)));
        assert!(w.throttle.has_trailing());
        assert!(!w.try_trailing(t0 + ms(100)));
        assert_eq!(w.trailing_deadline, Some(t0 + ms(150)));

        w.finish_run(t0 + ms(60));
        // 结束后，trailing 仍布置到延长后的终点。
        assert!(w.try_trailing(t0 + ms(150)));
        assert_eq!(w.invoke_starts, ["leading", "run_now", "trailing"]);
    }

    #[test]
    fn orch_cas_lost_leading_arms_trailing() {
        let mut w = WorkerOrch::new(ms(100));
        let t0 = Instant::now();

        // 外部 RUN_NOW 持有该标志。
        assert!(w.run_now(t0));

        // FS 触发决定 leading（在 throttle 看来调用之前处于窗口之外，
        // 但忙碌会强制 ArmTrailing）。走忙碌路径：is_running 为真。
        w.push_trigger();
        w.on_trigger(t0 + ms(5));
        assert!(w.throttle.has_trailing());
        assert_eq!(w.invoke_starts, ["run_now"]);

        // 显式的 CAS 失败路径：在窗口之外，throttle 决策里也不忙碌，
        // 但因为仍在运行所以 CAS 失败。
        w.is_running = false;
        w.throttle = InvokeThrottle::new(ms(100)); // fresh, outside window
        w.is_running = true; // held by "other" without mark — simulate race
        w.push_trigger();
        // 强制：手动模拟“窗口外 note_trigger + CAS 失败”
        let decision = w.throttle.note_trigger(t0 + ms(10), false);
        assert_eq!(decision, TriggerDecision::RunLeading);
        assert!(!w.cas_start("leading", t0 + ms(10))); // CAS lost
        w.throttle.arm_trailing();
        w.trailing_deadline = Some(t0 + ms(10));

        w.is_running = false;
        assert!(w.try_trailing(t0 + ms(10)));
        assert_eq!(w.invoke_starts.last().copied(), Some("trailing"));
    }

    #[test]
    fn orch_long_run_coalesces_channel_to_one_trailing() {
        let mut w = WorkerOrch::new(ms(50));
        let t0 = Instant::now();

        w.push_trigger();
        w.on_trigger(t0);
        assert_eq!(w.invoke_starts, ["leading"]);

        // 长时间运行期间，许多 FS 触发落在 channel 里。
        for _ in 0..8 {
            w.push_trigger();
        }
        // finish_run 归并 → 只布置一次 trailing。
        w.finish_run(t0 + ms(80)); // past window
        assert!(w.throttle.has_trailing() || w.trailing_deadline.is_some());
        assert_eq!(w.channel.len(), 0);

        let deadline = w
            .trailing_deadline
            .expect("expected trailing after coalesce");
        assert!(w.try_trailing(deadline.max(t0 + ms(80))));
        assert_eq!(w.invoke_starts, ["leading", "trailing"]);
        w.finish_run(deadline + ms(1));
        assert!(!w.throttle.has_trailing());
        assert_eq!(w.invoke_starts.len(), 2);
    }

    #[test]
    fn orch_immediate_ignore_then_run_now_no_double_lead() {
        // 模拟：ignore_initial 一直保持到 run_now；布置之前没有 FS leading。
        let mut w = WorkerOrch::new(ms(100));
        let t0 = Instant::now();
        let mut ignore = true;

        // 忽略期间的 FS 噪声——直接丢弃（不调 on_trigger）。
        assert!(ignore);

        assert!(w.run_now(t0));
        ignore = false;
        assert!(!ignore);

        // 布置之后，窗口内的 FS → 只做 trailing，不做第二次 leading。
        w.push_trigger();
        w.on_trigger(t0 + ms(10));
        assert_eq!(w.invoke_starts, ["run_now"]);
        assert!(matches!(
            w.trailing_deadline,
            Some(d) if d == t0 + ms(100)
        ));
    }
}
