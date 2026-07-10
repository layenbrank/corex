use std::fs;
use std::str::FromStr;

use anyhow::Result;
use chrono::Local;
use cron::Schedule as CronSchedule;
use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;

use crate::pipeline::config::{
    CONFIG_VERSION, PipelineConfig, PipelinesConfig, StepConfig, find_config_path, load_config,
    validate_config,
};
use crate::pipeline::context::PipelineContext;
use crate::pipeline::runner::run_pipeline;
use crate::schedule::schema::Args;

/// `corex schedule` 命令入口
pub fn run(args: &Args) -> Result<()> {
    match args {
        Args::Run => run_interactive(),
        Args::Generate => generate_config_template(),
        Args::Cron { config } => run_cron(config.as_deref()),
    }
}

/// 以守护进程模式运行，按 cron 表达式定时执行 Pipeline
fn run_cron(config_path: Option<&str>) -> Result<()> {
    let config_path = config_path
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

    let scheduled: Vec<(&PipelineConfig, CronSchedule)> = config
        .pipelines
        .iter()
        .filter_map(|p| {
            p.schedule
                .as_ref()
                .and_then(|expr| match CronSchedule::from_str(expr) {
                    Ok(sched) => Some((p, sched)),
                    Err(e) => {
                        eprintln!(
                            "  {} Pipeline '{}' cron 表达式无效 ({}): {}",
                            "×".red(),
                            p.id,
                            expr,
                            e
                        );
                        None
                    }
                })
        })
        .collect();

    if scheduled.is_empty() {
        anyhow::bail!(
            "配置文件中没有任何 Pipeline 设置了 schedule 字段\n\
             提示: 在 pipeline 配置中添加 `schedule: \"*/5 * * * *\"` 即可定时执行"
        );
    }

    print_banner("Corex · 定时调度器");

    println!(
        "  {} 已加载 {} 条定时 Pipeline（共 {} 条）\n",
        "✓".green().bold(),
        scheduled.len(),
        config.pipelines.len()
    );

    for (p, sched) in &scheduled {
        let next = sched.upcoming(Local).next();
        let next_str = next
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "无".to_string());
        println!(
            "  {} {} — schedule: {:?} — 下次执行: {}",
            "▸".cyan(),
            p.description.as_deref().unwrap_or(&p.id).bold(),
            p.schedule.as_deref().unwrap_or(""),
            next_str.dim()
        );
    }
    println!();
    println!("  {} 等待定时任务触发...（Ctrl+C 退出）\n", "⏳".yellow());

    let mut next_runs: std::collections::HashMap<String, chrono::DateTime<Local>> =
        std::collections::HashMap::new();

    for (pipeline, sched) in &scheduled {
        if let Some(next) = sched.upcoming(Local).next() {
            next_runs.insert(pipeline.id.clone(), next);
        }
    }

    loop {
        let now = Local::now();

        for (pipeline, sched) in &scheduled {
            let should_run = next_runs
                .get(&pipeline.id)
                .map(|&next| now >= next)
                .unwrap_or(false);

            if should_run {
                println!(
                    "\n  {} [{}] 定时触发: {}",
                    "⏰".yellow().bold(),
                    Local::now().format("%H:%M:%S").to_string().bold(),
                    pipeline
                        .description
                        .as_deref()
                        .unwrap_or(&pipeline.id)
                        .bold()
                );

                let mut ctx = PipelineContext::with_variables(config.variables.clone());
                if let Err(e) = run_pipeline(pipeline, &mut ctx) {
                    eprintln!("  {} Pipeline '{}' 执行失败: {}", "×".red(), pipeline.id, e);
                }

                if let Some(next) = sched.upcoming(Local).next() {
                    next_runs.insert(pipeline.id.clone(), next);
                    println!(
                        "  {} 下次执行: {}",
                        "⏳".yellow(),
                        next.format("%Y-%m-%d %H:%M:%S").to_string().dim()
                    );
                }
                println!();
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn run_interactive() -> Result<()> {
    let path = find_config_path();

    if !path.exists() {
        anyhow::bail!(
            "配置文件未找到：{}，请先运行 `corex schedule generate` 生成配置",
            path.display()
        );
    }

    let config = load_config(&path)?;
    validate_config(&config)?;

    if config.pipelines.is_empty() {
        anyhow::bail!("配置文件中没有找到有效的 Pipeline");
    }

    print_banner("Corex · 任务调度器");

    let mut labels: Vec<String> = config
        .pipelines
        .iter()
        .map(|p| {
            let desc = p.description.as_deref().unwrap_or(&p.id);
            format!(
                "{} {} ({} 步)",
                "▶".green().bold(),
                desc.bold(),
                p.steps.len(),
            )
        })
        .collect();
    labels.push(format!("{} {}", "↩".dim(), "返回".dim()));

    let pipeline_idx = dialoguer::Select::with_theme(&ColorfulTheme::default())
        .with_prompt("选择 Pipeline")
        .items(&labels)
        .default(0)
        .interact()?;

    if pipeline_idx >= config.pipelines.len() {
        return Ok(());
    }

    let pipeline = &config.pipelines[pipeline_idx];
    let mut ctx = PipelineContext::with_variables(config.variables.clone());
    run_pipeline(pipeline, &mut ctx)
}

fn new_step(
    id: String,
    module: &str,
    description: Option<String>,
    params: serde_json::Value,
) -> StepConfig {
    StepConfig {
        id,
        module: module.to_string(),
        description,
        depends_on: vec![],
        when: None,
        retry: None,
        params,
    }
}

fn generate_config_template() -> Result<()> {
    print_banner("Corex · 配置向导");
    let path = find_config_path();

    if path.exists() {
        println!(
            "  {} 配置文件已存在：{}",
            "→".yellow(),
            path.display().to_string().dim()
        );
        let confirmed = dialoguer::Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("是否覆盖")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !confirmed {
            println!("  {} 已取消。", "×".red());
            return Ok(());
        }
    }

    let pipeline_id: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Pipeline ID（英文，无空格）")
        .default("default".to_string())
        .interact_text()?;

    let pipeline_desc: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Pipeline 描述")
        .allow_empty(true)
        .interact_text()?;

    let schedule_input: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
        .with_prompt("定时调度 cron 表达式（留空则不定时执行，如 */5 * * * *）")
        .allow_empty(true)
        .interact_text()?;
    let schedule = if schedule_input.is_empty() {
        None
    } else {
        Some(schedule_input)
    };

    let task_types = [
        "  复制目录      copy",
        "  路径生成      generate path",
        "  UUID 生成     generate uuid",
        "  压缩打包      compression zip",
        "  解压缩        compression unzip",
        "  图片处理      shade",
        "  清理删除      scrub",
        "  截图          screenshot",
        "  环境初始化    bootstrap",
    ];
    println!();
    let selections = dialoguer::MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("选择要添加的步骤类型（按顺序）")
        .items(task_types)
        .interact()?;

    if selections.is_empty() {
        println!("  {} 未选择任何步骤，已取消。", "×".red());
        return Ok(());
    }

    let mut steps: Vec<StepConfig> = Vec::new();

    for (seq, &sel) in selections.iter().enumerate() {
        let step_id = format!("step_{}", seq + 1);

        let step = match sel {
            0 => {
                println!("\n  {} 复制步骤", "▸".cyan());
                let from: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("源路径")
                    .interact_text()?;
                let to: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("目标路径")
                    .interact_text()?;
                new_step(
                    step_id,
                    "copy",
                    Some("复制任务".to_string()),
                    serde_json::json!({ "from": from, "to": to, "empty": false, "includes": [], "excludes": [] }),
                )
            }
            1 => {
                println!("\n  {} 路径生成步骤", "▸".cyan());
                let from: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("源目录")
                    .interact_text()?;
                let to: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("输出文件路径")
                    .interact_text()?;
                let transform: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("转换规则")
                    .interact_text()?;
                new_step(
                    step_id,
                    "generate",
                    Some("路径生成任务".to_string()),
                    serde_json::json!({
                        "Path": {
                            "from": from, "to": to, "transform": transform,
                            "index": 0, "separator": "/", "pad": false,
                            "includes": [], "excludes": [], "uppercase": []
                        }
                    }),
                )
            }
            2 => {
                println!("\n  {} UUID 生成步骤", "▸".cyan());
                let count: usize = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("生成数量")
                    .default(1usize)
                    .interact_text()?;
                new_step(
                    step_id,
                    "generate",
                    Some("UUID 生成任务".to_string()),
                    serde_json::json!({ "Uuid": { "count": count, "uppercase": false } }),
                )
            }
            3 => {
                println!("\n  {} 压缩步骤", "▸".cyan());
                let from: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("源路径（目录）")
                    .interact_text()?;
                let to: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("输出压缩包路径")
                    .interact_text()?;
                new_step(
                    step_id,
                    "compression",
                    Some("压缩任务".to_string()),
                    serde_json::json!({
                        "Compress": {
                            "scheme": {
                                "Zip": { "from": from, "to": to, "level": 6, "method": "deflated" }
                            }
                        }
                    }),
                )
            }
            4 => {
                println!("\n  {} 解压缩步骤", "▸".cyan());
                let from: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("ZIP 文件路径")
                    .interact_text()?;
                let to: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("解压目标目录")
                    .interact_text()?;
                new_step(
                    step_id,
                    "compression",
                    Some("解压缩任务".to_string()),
                    serde_json::json!({
                        "Decompress": {
                            "scheme": {
                                "Zip": { "from": from, "to": to }
                            }
                        }
                    }),
                )
            }
            5 => {
                println!("\n  {} 图片处理步骤", "▸".cyan());
                let from: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("输入图片路径或目录")
                    .interact_text()?;
                let to: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("输出路径")
                    .interact_text()?;
                let format: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("输出格式（png/jpg/webp/bmp，留空自动推断）")
                    .allow_empty(true)
                    .interact_text()?;
                let quality: u8 = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("输出质量 1-100（100=无损）")
                    .default(100u8)
                    .interact_text()?;
                let format_val = if format.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(format)
                };
                new_step(
                    step_id,
                    "shade",
                    Some("图片处理任务".to_string()),
                    serde_json::json!({
                        "from": from, "to": to, "format": format_val, "quality": quality
                    }),
                )
            }
            6 => {
                println!("\n  {} 清理步骤", "▸".cyan());
                let source: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("目标路径")
                    .interact_text()?;
                let target: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("要删除的目标名称")
                    .interact_text()?;
                let recursive: bool = dialoguer::Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("递归删除")
                    .default(true)
                    .interact()
                    .unwrap_or(true);
                new_step(
                    step_id,
                    "scrub",
                    Some("清理任务".to_string()),
                    serde_json::json!({
                        "source": source, "target": target, "recursive": recursive
                    }),
                )
            }
            7 => {
                println!("\n  {} 截图步骤", "▸".cyan());
                let to: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("输出目录")
                    .interact_text()?;
                new_step(
                    step_id,
                    "screenshot",
                    Some("截图任务".to_string()),
                    serde_json::json!({ "Capture": { "to": to } }),
                )
            }
            8 => {
                println!("\n  {} 环境初始化步骤", "▸".cyan());
                let action_idx = dialoguer::Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("操作类型")
                    .items([
                        "env（设置环境变量）",
                        "inspect（检查环境）",
                        "force（强制重新设置）",
                    ])
                    .default(1)
                    .interact()?;
                let variant = match action_idx {
                    0 => "Env",
                    2 => "Force",
                    _ => "Inspect",
                };
                new_step(
                    step_id,
                    "bootstrap",
                    Some("环境初始化".to_string()),
                    serde_json::json!({ variant: serde_json::Value::Object(Default::default()) }),
                )
            }
            _ => continue,
        };

        steps.push(step);
    }

    // 为顺序步骤添加隐式 depends_on（除第一步外依赖前一步）
    for i in 1..steps.len() {
        if steps[i].depends_on.is_empty() {
            steps[i].depends_on = vec![steps[i - 1].id.clone()];
        }
    }

    let config = PipelinesConfig {
        version: CONFIG_VERSION,
        variables: std::collections::HashMap::new(),
        pipelines: vec![PipelineConfig {
            id: pipeline_id,
            description: if pipeline_desc.is_empty() {
                None
            } else {
                Some(pipeline_desc)
            },
            schedule,
            watch: None,
            steps,
        }],
    };

    let content = serde_yml::to_string(&config)?;
    let content = format!("# Corex Pipeline v3 配置文件\n{content}");

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &content)?;

    println!(
        "\n  {} 配置已生成：{}",
        "✓".green().bold(),
        path.display().to_string().bold()
    );
    Ok(())
}

fn print_banner(title: &str) {
    let width: usize = 54;
    let title_len = title.chars().count();
    let pad_total = width.saturating_sub(title_len + 2);
    let pad_left = pad_total / 2;
    let pad_right = pad_total - pad_left;
    println!();
    println!("{}", format!("╭{}╮", "─".repeat(width)).cyan().bold());
    println!(
        "{}",
        format!(
            "│{}{}{}│",
            " ".repeat(pad_left),
            title,
            " ".repeat(pad_right)
        )
        .cyan()
        .bold()
    );
    println!("{}", format!("╰{}╯", "─".repeat(width)).cyan().bold());
    println!();
}
