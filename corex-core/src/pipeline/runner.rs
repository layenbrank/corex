use anyhow::Result;
use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;

use crate::runtime::{self, merge_variables};

use super::config::{
    PipelineArgs, PipelineConfig, ValidateReport, find_config_path, load_config, validate_config,
};
use super::step_params::redact_sensitive_params;
use super::trigger::{self, label};

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

    if runtime::is_json_output() && !args.once && pipeline.triggers().any() {
        anyhow::bail!("守护模式不支持 --format json，请使用 --once");
    }

    trigger::run(pipeline, &config, &config_path, args)
}

pub fn run_pipeline(pipeline: &PipelineConfig, ctx: &mut super::context::PipelineContext) -> Result<()> {
    let report = super::orchestrator::run_pipeline(pipeline, ctx)?;
    if report.status == super::report::RunStatus::Failed {
        return Err(report.into_err());
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
            let badge = label(p);
            if badge.is_empty() {
                format!("▶ {desc} ({} 步)", p.steps.len())
            } else {
                format!("▶ {desc} ({badge} · {} 步)", p.steps.len())
            }
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
    let badge = label(pipeline);
    if !badge.is_empty() {
        println!("  触发器: {badge}");
    }
    if let Some(ref sched) = pipeline.schedule {
        println!("  定时调度: {sched}");
    }
    if let Some(ref watch) = pipeline.watch {
        println!(
            "  文件监听: {} (debounce {}ms)",
            watch.paths.join(", "),
            watch.debounce_ms
        );
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
