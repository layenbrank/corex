use std::time::Instant;

use anyhow::{Context, Result};
use crossterm::style::Stylize;
use tokio::task::JoinSet;
use tracing::info_span;

use crate::invoke::Artifact;
use crate::runtime;

use super::config::{PipelineConfig, StepConfig};
use super::context::PipelineContext;
use super::graph::StageGraph;
use super::report::{RunReport, RunStatus, StepReport, StepStatus};
use super::stream::{run_batch_stage, run_path_stream_blocking};

type StepOutcome = (Artifact, u64, u64, StepStatus, Option<String>);

/// 执行一条 Pipeline（DAG 分层 + when/retry）；失败时仍返回完整 RunReport
pub fn run_pipeline(pipeline: &PipelineConfig, ctx: &mut PipelineContext) -> Result<RunReport> {
    let started = Instant::now();
    let mut report = RunReport::new(&pipeline.id);
    let graph = StageGraph::from_pipeline(pipeline)?;
    let layers = graph.execution_layers()?;

    if !runtime::is_quiet() && !runtime::is_json_output() {
        println!(
            "\n  {} 执行 Pipeline：{}\n",
            "▶".green().bold(),
            pipeline
                .description
                .as_deref()
                .unwrap_or(&pipeline.id)
                .bold()
        );
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("创建 tokio runtime 失败")?;

    rt.block_on(async {
        'layers: for layer in layers {
            if layer.len() == 1 {
                let step_id = &layer[0];
                let step = graph
                    .step_by_id(pipeline, step_id)
                    .context("步骤未找到")?
                    .clone();
                let outcome = execute_step_with_retry(&step, ctx).await?;
                if apply_outcome(&mut report, &step, outcome, ctx) {
                    break 'layers;
                }
            } else {
                if run_parallel_layer(&layer, pipeline, &graph, ctx, &mut report).await? {
                    break 'layers;
                }
            }
        }
        anyhow::Ok(())
    })?;

    report.duration_ms = started.elapsed().as_millis() as u64;

    if report.status == RunStatus::Failed {
        if !runtime::is_quiet() && !runtime::is_json_output() {
            eprintln!(
                "\n  {} Pipeline 执行失败（{} ms）",
                "×".red().bold(),
                report.duration_ms
            );
            eprint_failed(&report);
        }
    } else if !runtime::is_quiet() && !runtime::is_json_output() {
        println!(
            "\n  {} Pipeline 执行完成（{} ms）",
            "✓".green().bold(),
            report.duration_ms
        );
    }

    Ok(report)
}

fn eprint_failed(report: &RunReport) {
    for step in &report.steps {
        if step.status == StepStatus::Failed {
            if let Some(ref err) = step.error {
                eprintln!(
                    "     {} [{}] {}",
                    "×".red(),
                    step.id.as_str().bold(),
                    err
                );
            }
        }
    }
}

/// 并行执行同层步骤；任一步失败则 abort 其余任务。返回 true 表示本层失败。
async fn run_parallel_layer(
    layer: &[String],
    pipeline: &PipelineConfig,
    graph: &StageGraph,
    ctx: &mut PipelineContext,
    report: &mut RunReport,
) -> Result<bool> {
    let mut set: JoinSet<Result<(StepConfig, StepOutcome)>> = JoinSet::new();
    for step_id in layer {
        let step = graph
            .step_by_id(pipeline, step_id)
            .context("步骤未找到")?
            .clone();
        let vars = ctx.variables.clone();
        let artifacts = ctx.step_artifacts.clone();
        set.spawn(async move {
            let mut local = PipelineContext {
                variables: vars,
                step_artifacts: artifacts,
            };
            let outcome = execute_step_with_retry(&step, &mut local).await?;
            Ok((step, outcome))
        });
    }

    let mut layer_failed = false;
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok((step, outcome))) => {
                if apply_outcome(report, &step, outcome, ctx) {
                    layer_failed = true;
                    set.abort_all();
                }
            }
            Ok(Err(e)) => {
                report.fail();
                set.abort_all();
                return Err(e);
            }
            Err(e) => {
                report.fail();
                set.abort_all();
                return Err(e.into());
            }
        }
    }
    while set.join_next().await.is_some() {}
    Ok(layer_failed)
}

/// 将步骤结果写入 report 与上下文；返回 true 表示该步失败。
fn apply_outcome(
    report: &mut RunReport,
    step: &StepConfig,
    outcome: StepOutcome,
    ctx: &mut PipelineContext,
) -> bool {
    let (artifact, items, duration_ms, status, err) = outcome;
    let is_success = status == StepStatus::Success;
    let is_failed = status == StepStatus::Failed;
    if is_failed {
        report.fail();
    } else if is_success {
        ctx.set_artifact(step.id.clone(), artifact.clone());
    }
    report.steps.push(StepReport {
        id: step.id.clone(),
        module: step.module.clone(),
        status,
        artifact: if is_success { Some(artifact) } else { None },
        items,
        duration_ms,
        error: err,
    });
    is_failed
}

async fn execute_step_with_retry(
    step: &StepConfig,
    ctx: &mut PipelineContext,
) -> Result<StepOutcome> {
    let max = step.retry.as_ref().map(|r| r.max).unwrap_or(1).max(1);
    let backoff = step.retry.as_ref().map(|r| r.backoff_ms).unwrap_or(0);
    let mut last_err = None;
    for attempt in 0..max {
        if attempt > 0 && backoff > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;
        }
        match execute_step_once(step, ctx).await {
            Ok(outcome) => return Ok(outcome),
            Err(e) => last_err = Some(e),
        }
    }
    Ok((
        Artifact::default(),
        0,
        0,
        StepStatus::Failed,
        last_err.map(|e| e.to_string()),
    ))
}

async fn execute_step_once(step: &StepConfig, ctx: &mut PipelineContext) -> Result<StepOutcome> {
    let span = info_span!("pipeline_step", step_id = %step.id, module = %step.module);
    let _enter = span.enter();
    execute_step_blocking(step, ctx)
}

fn execute_step_blocking(step: &StepConfig, ctx: &mut PipelineContext) -> Result<StepOutcome> {
    let started = Instant::now();

    if let Some(ref when) = step.when {
        if !ctx.eval_when(when) {
            return Ok((
                Artifact::default(),
                0,
                started.elapsed().as_millis() as u64,
                StepStatus::Skipped,
                None,
            ));
        }
    }

    if !runtime::is_quiet() && !runtime::is_json_output() {
        println!(
            "  {} {}",
            "▸".cyan(),
            step.description.as_deref().unwrap_or(&step.module)
        );
    }

    let (artifact, items) = if step.module == "generate"
        && step.action.as_deref() == Some("path")
    {
        let path_args = parse_path_args(step, ctx)?;
        run_path_stream_blocking(&path_args)?
    } else {
        run_batch_stage(step, ctx)?
    };

    Ok((
        artifact,
        items,
        started.elapsed().as_millis() as u64,
        StepStatus::Success,
        None,
    ))
}

fn parse_path_args(
    step: &StepConfig,
    ctx: &PipelineContext,
) -> Result<crate::generate::schema::PathArgs> {
    let parsed = ctx.parse_value(&step.params);
    let wire = crate::invoke::WireArgs {
        action: step.action.clone(),
        format: step.format.clone(),
        algorithm: step.algorithm.clone(),
        flags: parsed,
    };
    let typed = crate::invoke::assemble_typed(&step.module, &wire)?;
    let args: crate::generate::schema::Args = serde_json::from_value(typed)?;
    match args {
        crate::generate::schema::Args::Path(p) => Ok(p),
        _ => anyhow::bail!("generate 流式 stage 需要 action: path"),
    }
}
