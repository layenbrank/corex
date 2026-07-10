use anyhow::Result;
use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;

use crate::runtime::{self, merge_variables};

use super::config::{
    PipelineArgs, PipelineConfig, ValidateReport, find_config_path, load_config, validate_config,
};
use super::context::PipelineContext;
use super::orchestrator::run_pipeline as orchestrate;
use super::report::write_report;
use super::step_params::redact_sensitive_params;

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

    let mut config = load_config(&config_path)?;
    config.variables = merge_variables(config.variables.clone(), &args.define);

    if let Err(e) = validate_config(&config) {
        if runtime::is_json_output() {
            let report = ValidateReport {
                ok: false,
                pipeline_count: config.pipelines.len(),
                errors: vec![e.to_string()],
            };
            runtime::state().emitter.json(&report)?;
        }
        return Err(e);
    }

    if args.validate {
        let report = ValidateReport::success(config.pipelines.len());
        if runtime::is_json_output() {
            runtime::state().emitter.json(&report)?;
        } else if !runtime::is_quiet() {
            println!(
                "  {} 配置验证通过 ({} 条 pipeline)",
                "✓".green().bold(),
                config.pipelines.len()
            );
        }
        return Ok(());
    }

    let pipeline = select_pipeline(&config, args)?;

    if args.dry_run {
        dry_run_pipeline(pipeline);
        return Ok(());
    }

    let mut ctx = PipelineContext::with_variables(config.variables.clone());
    let report = orchestrate(pipeline, &mut ctx)?;

    if runtime::is_json_output() {
        runtime::state().emitter.json(&report)?;
    }

    if let Some(ref path) = args.report_file {
        write_report(path, &report)?;
    }

    if report.status == super::report::RunStatus::Failed {
        anyhow::bail!("Pipeline 执行失败");
    }

    Ok(())
}

pub fn run_pipeline(pipeline: &PipelineConfig, ctx: &mut PipelineContext) -> Result<()> {
    let report = orchestrate(pipeline, ctx)?;
    if report.status == super::report::RunStatus::Failed {
        anyhow::bail!("Pipeline 执行失败");
    }
    Ok(())
}

fn select_pipeline<'a>(
    config: &'a super::config::PipelinesConfig,
    args: &PipelineArgs,
) -> Result<&'a PipelineConfig> {
    if let Some(ref id) = args.id {
        return config
            .pipelines
            .iter()
            .find(|p| p.id == *id)
            .ok_or_else(|| anyhow::anyhow!("未找到 Pipeline: {id}"));
    }
    if config.pipelines.len() == 1 {
        return Ok(&config.pipelines[0]);
    }
    let labels: Vec<String> = config
        .pipelines
        .iter()
        .map(|p| {
            let desc = p.description.as_deref().unwrap_or(&p.id);
            format!("▶ {desc} ({} 步)", p.steps.len())
        })
        .collect();
    let idx = dialoguer::Select::with_theme(&ColorfulTheme::default())
        .with_prompt("选择 Pipeline")
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(&config.pipelines[idx])
}

fn dry_run_pipeline(pipeline: &PipelineConfig) {
    if runtime::is_quiet() || runtime::is_json_output() {
        return;
    }
    println!(
        "\n  {} Dry-run：{}\n",
        "◇".yellow().bold(),
        pipeline
            .description
            .as_deref()
            .unwrap_or(&pipeline.id)
            .bold()
    );
    if let Some(ref sched) = pipeline.schedule {
        println!("  定时调度: {sched}");
    }
    println!("  步骤数: {}\n", pipeline.steps.len());
    for (i, step) in pipeline.steps.iter().enumerate() {
        println!(
            "  [{}] {} — module={}",
            (i + 1).to_string().bold(),
            step.id,
            step.module,
        );
        if let Some(ref desc) = step.description {
            println!("       描述: {desc}");
        }
        if !step.depends_on.is_empty() {
            println!("       depends_on: {:?}", step.depends_on);
        }
        let display_params = redact_sensitive_params(&step.params);
        println!(
            "       参数: {}",
            serde_json::to_string_pretty(&display_params).unwrap_or_default()
        );
        println!();
    }
}
