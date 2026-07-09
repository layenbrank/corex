use anyhow::Result;
use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;

use crate::pipeline::config::{
    ExecutionMode, PipelineArgs, PipelineConfig, StepConfig, find_config_path, load_config,
    validate_config,
};
use crate::pipeline::context::PipelineContext;
use crate::tasks::{TaskOutput, create_executor};

// ─── Pipeline 子命令入口 ────────────────────────────────────────────────────

/// `corex pipeline` 命令处理
pub fn run(args: &PipelineArgs) -> Result<()> {
    let config_path = args
        .config
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(find_config_path);

    if !config_path.exists() {
        anyhow::bail!(
            "配置文件未找到：{}，请先运行 `corex schedule generate` 生成配置",
            config_path.display()
        );
    }

    let config = load_config(&config_path)?;
    validate_config(&config)?;

    if args.validate {
        println!(
            "  {} 配置验证通过 ({} 条 pipeline)",
            "✓".green().bold(),
            config.pipelines.len()
        );
        return Ok(());
    }

    // 选择 pipeline
    let pipeline = if let Some(ref id) = args.id {
        config
            .pipelines
            .iter()
            .find(|p| p.id == *id)
            .ok_or_else(|| anyhow::anyhow!("未找到 Pipeline: {}", id))?
    } else if config.pipelines.len() == 1 {
        &config.pipelines[0]
    } else {
        // 交互选择
        let labels: Vec<String> = config
            .pipelines
            .iter()
            .map(|p| {
                let desc = p.description.as_deref().unwrap_or(&p.id);
                format!("▶ {} ({} 步, {:?})", desc, p.steps.len(), p.mode)
            })
            .collect();

        let idx = dialoguer::Select::with_theme(&ColorfulTheme::default())
            .with_prompt("选择 Pipeline")
            .items(&labels)
            .default(0)
            .interact()?;

        &config.pipelines[idx]
    };

    let mut ctx = PipelineContext::with_variables(config.variables.clone());

    if args.dry_run {
        dry_run_pipeline(pipeline, &ctx);
        return Ok(());
    }

    run_pipeline(pipeline, &mut ctx)
}

// ─── Pipeline 执行引擎 ──────────────────────────────────────────────────────

/// 执行一条 Pipeline
pub fn run_pipeline(pipeline: &PipelineConfig, ctx: &mut PipelineContext) -> Result<()> {
    println!(
        "\n  {} 执行 Pipeline：{}\n",
        "▶".green().bold(),
        pipeline
            .description
            .as_deref()
            .unwrap_or(&pipeline.id)
            .bold()
    );

    match pipeline.mode {
        ExecutionMode::Sequential => run_sequential(pipeline, ctx),
        ExecutionMode::Parallel => run_parallel(pipeline, ctx),
    }
}

/// Sequential 模式：顺序执行，支持步骤间协作
fn run_sequential(pipeline: &PipelineConfig, ctx: &mut PipelineContext) -> Result<()> {
    for (i, step) in pipeline.steps.iter().enumerate() {
        let label = step_label(step, i);
        println!(
            "  {} [{}] {}",
            "▸".cyan(),
            (i + 1).to_string().bold(),
            label
        );

        match execute_step(step, ctx) {
            Ok(output) => {
                ctx.set_step_output(step.id.clone(), output);
            }
            Err(e) => {
                eprintln!(
                    "  {} 步骤 {} [{}] 失败: {}",
                    "×".red(),
                    (i + 1).to_string().bold(),
                    step.id,
                    e
                );
                return Err(e);
            }
        }
    }

    println!(
        "\n  {} Pipeline 执行完成（共 {} 步）",
        "✓".green().bold(),
        pipeline.steps.len()
    );
    Ok(())
}

/// Parallel 模式：并发执行，步骤间独立
fn run_parallel(pipeline: &PipelineConfig, ctx: &mut PipelineContext) -> Result<()> {
    println!(
        "  {} 并发执行 {} 个步骤\n",
        "⚡".yellow().bold(),
        pipeline.steps.len()
    );

    // 使用 tokio 并发执行
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let mut handles = Vec::new();

        for (i, step) in pipeline.steps.iter().enumerate() {
            let step = step.clone();
            let vars = ctx.variables.clone();
            let step_id = step.id.clone();

            let handle = tokio::spawn(async move {
                let mut step_ctx = PipelineContext::with_variables(vars);
                println!(
                    "  {} [{}] {} (parallel)",
                    "▸".cyan(),
                    (i + 1).to_string().bold(),
                    step_label(&step, i)
                );
                execute_step(&step, &mut step_ctx)
            });

            handles.push((step_id, handle));
        }

        for (step_id, handle) in handles {
            match handle.await {
                Ok(Ok(output)) => {
                    ctx.set_step_output(step_id, output);
                }
                Ok(Err(e)) => {
                    eprintln!("  {} 步骤 [{}] 失败: {}", "×".red(), step_id, e);
                    return Err(e);
                }
                Err(e) => {
                    eprintln!("  {} 步骤 [{}] 被取消: {}", "×".red(), step_id, e);
                    return Err(anyhow::anyhow!("步骤 {} 被取消", step_id));
                }
            }
        }

        Ok(())
    })?;

    println!(
        "\n  {} Pipeline 并发执行完成（共 {} 步）",
        "✓".green().bold(),
        pipeline.steps.len()
    );
    Ok(())
}

/// 执行单个步骤
fn execute_step(step: &StepConfig, ctx: &mut PipelineContext) -> Result<TaskOutput> {
    let executor = create_executor(&step.module, step.action.as_deref()).ok_or_else(|| {
        anyhow::anyhow!(
            "未知的模块/动作组合: module={}, action={:?}",
            step.module,
            step.action
        )
    })?;

    // 解析 params 中的变量引用
    let resolved_params = ctx.resolve_value(&step.params);

    executor.execute(&resolved_params, ctx)
}

// ─── Dry-run ────────────────────────────────────────────────────────────────

fn dry_run_pipeline(pipeline: &PipelineConfig, _ctx: &PipelineContext) {
    println!(
        "\n  {} Dry-run：{}\n",
        "◇".yellow().bold(),
        pipeline
            .description
            .as_deref()
            .unwrap_or(&pipeline.id)
            .bold()
    );

    println!("  模式: {:?}", pipeline.mode);
    if let Some(ref sched) = pipeline.schedule {
        println!("  定时调度: {}", sched);
    }
    println!("  步骤数: {}\n", pipeline.steps.len());

    for (i, step) in pipeline.steps.iter().enumerate() {
        println!(
            "  [{}] {} — module={}, action={:?}",
            (i + 1).to_string().bold(),
            step.id,
            step.module,
            step.action,
        );
        if let Some(ref desc) = step.description {
            println!("       描述: {}", desc);
        }
        println!(
            "       参数: {}",
            serde_json::to_string_pretty(&step.params).unwrap_or_default()
        );
        println!();
    }
}

// ─── UI 辅助函数 ────────────────────────────────────────────────────────────

fn step_label(step: &StepConfig, _index: usize) -> String {
    let desc = step.description.as_deref().unwrap_or(&step.module);

    let tag = match step.module.as_str() {
        "copy" => format!("[{}]", "复制".cyan().bold()),
        "scrub" => format!("[{}]", "清理".red().bold()),
        "compression" => format!(
            "[{}]",
            step.action.as_deref().unwrap_or("zip").yellow().bold()
        ),
        "generate" => format!(
            "[{}]",
            step.action.as_deref().unwrap_or("path").green().bold()
        ),
        "screenshot" => format!("[{}]", "截图".magenta().bold()),
        "shade" => format!("[{}]", "图片".cyan().bold()),
        "bootstrap" => format!("[{}]", "环境".blue().bold()),
        other => format!("[{}]", other.bold()),
    };

    format!("{} {}", tag, desc)
}
