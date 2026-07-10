use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crossterm::style::Stylize;

use crate::pipeline::config::PipelineConfig;
use crate::pipeline::context::PipelineContext;
use crate::pipeline::runner::run_pipeline;

/// 正在执行的 pipeline id 集合（watch / cron 共享）
pub type RunningSet = Arc<Mutex<HashSet<String>>>;

pub fn new_set() -> RunningSet {
    Arc::new(Mutex::new(HashSet::new()))
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
}
