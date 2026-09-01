//! Lodash-like invoke throttle for watch pipelines (`leading: true`, `trailing: true`).
//!
//! Pipeline: FS events → **notify_debouncer_full** (FS quiet-period debounce, not a
//! lodash debounce port) → logical triggers → **this throttle** → `run_directive_file`.
//!
//! Timing anchor: the throttle window starts at **invoke start** (call time), matching
//! common lodash throttle “invocation moment” semantics. YAML field: `throttle_ms`.

use std::time::{Duration, Instant};

/// Decision after a logical trigger (post-debounce) arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerDecision {
    /// Outside the throttle window: invoke immediately (leading edge).
    RunLeading,
    /// Inside the window: arm trailing; wake at `until` (= last invoke start + interval).
    ArmTrailing { until: Instant },
}

/// Rate-limits pipeline invokes: at most one leading and one trailing per window.
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

    /// Record a logical trigger at `now`.
    ///
    /// When `is_busy` (another invoke holds `is_running`), always arm trailing and
    /// never attempt a concurrent leading — CAS elsewhere still enforces single flight.
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

    /// Call when an invoke actually begins (leading, trailing, or RUN_NOW).
    ///
    /// Clears trailing: the current invoke absorbs the pending edge. New triggers
    /// during this run re-arm trailing via [`note_trigger`].
    pub fn mark_invoke_start(&mut self, at: Instant) {
        self.last_invoke_start = Some(at);
        self.trailing_pending = false;
    }

    /// `RUN_NOW` / `immediate`: refresh the throttle window so the next FS trigger
    /// does not immediately leading-fire again.
    ///
    /// Does **not** clear `trailing_pending` — FS triggers already armed still get
    /// one trailing after the new window (or ASAP if the window already elapsed).
    pub fn record_external_invoke(&mut self, at: Instant) {
        self.last_invoke_start = Some(at);
    }

    pub fn arm_trailing(&mut self) {
        self.trailing_pending = true;
    }

    /// Consume trailing flag when the deadline is due (or run was deferred).
    pub fn take_trailing(&mut self) -> bool {
        let had = self.trailing_pending;
        self.trailing_pending = false;
        had
    }
}

/// Sleep until `until`, re-checking `window_end` after wake so `RUN_NOW` can extend
/// the window without a spurious early trailing invoke.
pub async fn wait_for_trailing_deadline(
    throttle: &std::sync::Mutex<InvokeThrottle>,
    mut until: Instant,
) {
    loop {
        let now = Instant::now();
        if now >= until {
            // Window may have moved (external invoke); reschedule if still inside.
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
        // No further triggers → no trailing.
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
        // Never invoked; busy (e.g. RUN_NOW) → trailing ASAP.
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
        // RUN_NOW / immediate refreshes last_invoke.
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
        // RUN_NOW mid-window: keep trailing armed, move window.
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

    /// Simulates worker decisions: leading + in-window bursts → one trailing only.
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
        // No extra events after trailing → still quiet.
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
            // Mimic RUN_NOW refreshing last_invoke mid-wait.
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
        // After wait returns, we should be at/past the (possibly extended) window.
        assert!(throttle.lock().unwrap().is_outside_window(Instant::now()));
    }

    /// Thin sync stand-in for the engine worker decision path (no FS / no real run).
    struct WorkerOrch {
        throttle: InvokeThrottle,
        is_running: bool,
        trailing_deadline: Option<Instant>,
        /// Successful invoke starts (leading, trailing, or external).
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

        /// Mirror: recv + coalesce + note_trigger + maybe leading.
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

        /// RUN_NOW / immediate: CAS + record_external (does not clear trailing).
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
            // trailing_after_run: coalesce leftover channel into ≤1 trailing.
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

        /// Fire trailing when due (after wait); respects extended window.
        fn try_trailing(&mut self, now: Instant) -> bool {
            let Some(until) = self.trailing_deadline else {
                return false;
            };
            if now < until {
                return false;
            }
            // wait_for_trailing_deadline refresh
            if let Some(end) = self.throttle.window_end().filter(|&e| now < e) {
                self.trailing_deadline = Some(end);
                return false;
            }
            self.trailing_deadline = None;
            if !self.throttle.take_trailing() {
                return false;
            }
            while self.is_running {
                // Busy-wait path: absorb channel as busy trailing.
                let _ = self.drain_channel();
                let _ = self.throttle.note_trigger(now, true);
                return false; // caller must retry after finish
            }
            if !self.throttle.is_outside_window(now) {
                if let Some(end) = self.throttle.window_end() {
                    self.throttle.arm_trailing();
                    self.trailing_deadline = Some(end);
                    return false;
                }
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

        // Burst inside window → one trailing arm only.
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

        // RUN_NOW mid-wait extends window; trailing must not fire at old deadline.
        assert!(w.run_now(t0 + ms(50)));
        assert!(w.throttle.has_trailing());
        assert!(!w.try_trailing(t0 + ms(100)));
        assert_eq!(w.trailing_deadline, Some(t0 + ms(150)));

        w.finish_run(t0 + ms(60));
        // After finish, trailing still armed to extended end.
        assert!(w.try_trailing(t0 + ms(150)));
        assert_eq!(w.invoke_starts, ["leading", "run_now", "trailing"]);
    }

    #[test]
    fn orch_cas_lost_leading_arms_trailing() {
        let mut w = WorkerOrch::new(ms(100));
        let t0 = Instant::now();

        // External RUN_NOW holds the flag.
        assert!(w.run_now(t0));

        // FS trigger decides leading (outside window from throttle's view before
        // note — but busy forces ArmTrailing). Use busy path: is_running true.
        w.push_trigger();
        w.on_trigger(t0 + ms(5));
        assert!(w.throttle.has_trailing());
        assert_eq!(w.invoke_starts, ["run_now"]);

        // Explicit CAS-lost path: outside window, not busy in throttle decision,
        // but CAS fails because still running.
        w.is_running = false;
        w.throttle = InvokeThrottle::new(ms(100)); // fresh, outside window
        w.is_running = true; // held by "other" without mark — simulate race
        w.push_trigger();
        // Force: manually simulate note_trigger outside + CAS fail
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

        // During long run, many FS triggers land in channel.
        for _ in 0..8 {
            w.push_trigger();
        }
        // finish_run coalesces → single trailing arm.
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
        // Simulates: ignore_initial held until run_now; no FS leading before arm.
        let mut w = WorkerOrch::new(ms(100));
        let t0 = Instant::now();
        let mut ignore = true;

        // FS noise while ignoring — dropped (no on_trigger).
        assert!(ignore);

        assert!(w.run_now(t0));
        ignore = false;
        assert!(!ignore);

        // After arm, in-window FS → trailing only, not second leading.
        w.push_trigger();
        w.on_trigger(t0 + ms(10));
        assert_eq!(w.invoke_starts, ["run_now"]);
        assert!(matches!(
            w.trailing_deadline,
            Some(d) if d == t0 + ms(100)
        ));
    }
}
