use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::style::Stylize;

use crate::pipeline::config::PipelineConfig;
use crate::pipeline::context::PipelineContext;
use crate::pipeline::runner::run_pipeline;

/// 正在执行的 pipeline id 集合（watch / cron 共享）
pub type RunningSet = Arc<Mutex<HashSet<String>>>;

/// 最近一次 pipeline 执行完成时间（watch 冷却抑制）
pub type LastFinished = Arc<Mutex<Option<Instant>>>;

pub fn new_set() -> RunningSet {
    Arc::new(Mutex::new(HashSet::new()))
}

pub fn new_last_finished() -> LastFinished {
    Arc::new(Mutex::new(None))
}

pub fn mark_finished(last_finished: &LastFinished) {
    *last_finished
        .lock()
        .expect("last_finished lock poisoned") = Some(Instant::now());
}

/// 是否仍处于 post-run 冷却窗口内
pub fn is_in_cooldown(last_finished: &LastFinished, cooldown: Duration) -> bool {
    let guard = last_finished
        .lock()
        .expect("last_finished lock poisoned");
    guard
        .is_some_and(|finished| finished.elapsed() < cooldown)
}

pub fn try_acquire(running: &RunningSet, pipeline_id: &str) -> bool {
    let mut guard = running.lock().expect("running lock poisoned");
    if guard.contains(pipeline_id) {
        false
    } else {
        guard.insert(pipeline_id.to_string());
        true
    }
}

pub fn release(running: &RunningSet, pipeline_id: &str) {
    running
        .lock()
        .expect("running lock poisoned")
        .remove(pipeline_id);
}

/// 后台执行 pipeline；若已在运行则跳过
pub fn spawn(
    running: RunningSet,
    pipeline: &PipelineConfig,
    variables: &HashMap<String, String>,
    reason: &str,
    last_finished: Option<LastFinished>,
) {
    let pipeline_id = pipeline.id.clone();
    if !try_acquire(&running, &pipeline_id) {
        eprintln!(
            "  {} Pipeline '{}' 正在执行，跳过本次{}触发",
            "⊘".yellow(),
            pipeline_id,
            reason
        );
        return;
    }

    let desc = pipeline
        .description
        .as_deref()
        .unwrap_or(&pipeline.id)
        .to_string();
    let pipeline = pipeline.clone();
    let variables = variables.clone();

    println!(
        "\n  {} [{}] {} 触发: {}",
        "⚡".yellow().bold(),
        chrono::Local::now().format("%H:%M:%S"),
        reason,
        desc.bold()
    );

    std::thread::spawn(move || {
        let mut ctx = PipelineContext::with_variables(variables);
        let result = run_pipeline(&pipeline, &mut ctx);
        release(&running, &pipeline_id);
        if let Some(last_finished) = last_finished {
            mark_finished(&last_finished);
        }

        match result {
            Ok(()) => println!("  {} Pipeline '{}' 执行完成\n", "✓".green(), pipeline_id),
            Err(e) => eprintln!(
                "  {} Pipeline '{}' 执行失败: {}\n",
                "×".red(),
                pipeline_id,
                e
            ),
        }
    });
}

/// 同步执行；若已在运行则跳过
pub fn run_sync(
    running: &RunningSet,
    pipeline: &PipelineConfig,
    variables: &HashMap<String, String>,
    reason: &str,
) {
    let pipeline_id = pipeline.id.clone();
    if !try_acquire(running, &pipeline_id) {
        eprintln!(
            "  {} Pipeline '{}' 正在执行，跳过本次{}触发",
            "⊘".yellow(),
            pipeline_id,
            reason
        );
        return;
    }

    let mut ctx = PipelineContext::with_variables(variables.clone());
    let result = run_pipeline(pipeline, &mut ctx);
    release(running, &pipeline_id);

    match result {
        Ok(()) => println!("  {} Pipeline '{}' 执行完成\n", "✓".green(), pipeline_id),
        Err(e) => eprintln!(
            "  {} Pipeline '{}' 执行失败: {}\n",
            "×".red(),
            pipeline_id,
            e
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_blocks_duplicate() {
        let running = new_set();
        assert!(try_acquire(&running, "a"));
        assert!(!try_acquire(&running, "a"));
        release(&running, "a");
        assert!(try_acquire(&running, "a"));
    }

    #[test]
    fn release_allows_reacquire() {
        let running = new_set();
        assert!(try_acquire(&running, "demo"));
        release(&running, "demo");
        assert!(try_acquire(&running, "demo"));
    }

    #[test]
    fn is_in_cooldown_after_mark_finished() {
        let last = new_last_finished();
        assert!(!is_in_cooldown(&last, Duration::from_millis(1000)));
        mark_finished(&last);
        assert!(is_in_cooldown(&last, Duration::from_millis(1000)));
    }
}
