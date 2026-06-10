use std::{
    fs,
    path::{Path, PathBuf},
};

use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;

use crate::schedule::schema::{Args, Pipeline, ScheduleConfig, Step};
use crate::schedule::pipeline::Context;
use crate::{compression, copy, generate, scrub};

pub fn run(args: &Args) -> anyhow::Result<()> {
    match args {
        Args::Run => run_schedule(),
        Args::Generate => generate_config(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 配置文件加载（支持 YAML / JSON）
// ─────────────────────────────────────────────────────────────────────────────

/// 查找配置文件，按优先级：.yaml > .yml > .json
fn config_path() -> PathBuf {
    let base = dirs::home_dir().expect("无法获取用户目录").join(".corex");
    let yaml = base.join("corex.config.yaml");
    if yaml.exists() {
        return yaml;
    }
    let yml = base.join("corex.config.yml");
    if yml.exists() {
        return yml;
    }
    base.join("corex.config.json")
}

fn load_config(path: &Path) -> anyhow::Result<ScheduleConfig> {
    let content =
        fs::read_to_string(path).map_err(|e| anyhow::anyhow!("读取配置文件失败: {}", e))?;
    match path.extension().and_then(|e| e.to_str()) {
        Some("yaml") | Some("yml") => {
            serde_yml::from_str(&content).map_err(|e| anyhow::anyhow!("解析 YAML 配置失败: {}", e))
        }
        _ => {
            serde_json::from_str(&content).map_err(|e| anyhow::anyhow!("解析 JSON 配置失败: {}", e))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline 执行引擎
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum ExecutionMode {
    All,
    From(usize),
    Selected(Vec<usize>),
}

/// 执行 pipeline，支持全部执行、从某步开始、选择性执行
fn run_pipeline(pipeline: &Pipeline, mode: &ExecutionMode) -> anyhow::Result<()> {
    let indices: Vec<usize> = collect_step_indices(pipeline, mode);
    if indices.is_empty() {
        anyhow::bail!("没有要执行的步骤");
    }

    println!(
        "\n  {} 执行流水线：{}\n",
        "▶".green().bold(),
        pipeline
            .description
            .as_deref()
            .unwrap_or(&pipeline.id)
            .bold()
    );

    let mut ctx = Context::new();

    for &i in &indices {
        let step = &pipeline.steps[i];
        let label = step_label(step, i);
        println!(
            "  {} [{}] {}",
            "▸".cyan(),
            (i + 1).to_string().bold(),
            label
        );

        if let Err(e) = execute_step(step, &mut ctx) {
            eprintln!(
                "  {} 步骤 {} 失败: {}",
                "×".red(),
                (i + 1).to_string().bold(),
                e
            );
            return Err(e);
        }
    }

    println!(
        "\n  {} 流水线执行完成（共 {} 步）",
        "✓".green().bold(),
        indices.len()
    );
    Ok(())
}

fn collect_step_indices(pipeline: &Pipeline, mode: &ExecutionMode) -> Vec<usize> {
    match mode {
        ExecutionMode::All => (0..pipeline.steps.len()).collect(),
        ExecutionMode::From(start) => (*start..pipeline.steps.len()).collect(),
        ExecutionMode::Selected(indices) => indices.clone(),
    }
}

/// 执行单个步骤并更新 Context
fn execute_step(step: &Step, ctx: &mut Context) -> anyhow::Result<()> {
    match step {
        Step::Copy(args) => {
            copy::service::execute(args, ctx)?;
        }
        Step::GeneratePath(args) => {
            generate::service::execute_path(args, ctx)?;
        }
        Step::GenerateUuid(args) => {
            generate::service::execute_uuid(args, ctx);
        }
        Step::Scrub(args) => {
            scrub::service::execute(args, ctx)?;
        }
        Step::Compression(args) => {
            compression::service::execute(args, ctx)?;
        }
    }
    Ok(())
}

fn step_label(step: &Step, _index: usize) -> String {
    match step {
        Step::Copy(args) => {
            let desc = args.description.as_deref().unwrap_or("复制");
            format!("{} {}", "[复制]".cyan().bold(), desc)
        }
        Step::GeneratePath(args) => {
            let desc = args.description.as_deref().unwrap_or("路径生成");
            format!("{} {}", "[路径]".green().bold(), desc)
        }
        Step::GenerateUuid(args) => {
            let desc = args.description.as_deref().unwrap_or("UUID 生成");
            format!("{} {}", "[UUID]".magenta().bold(), desc)
        }
        Step::Compression(args) => {
            let desc = args.description.as_deref().unwrap_or("压缩");
            format!("{} {}", "[压缩]".yellow().bold(), desc)
        }
        Step::Scrub(args) => {
            let desc = args.description.as_deref().unwrap_or(" scrub ");
            format!("{} {}", "[ scrub ]".red().bold(), desc)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// run_schedule：加载配置 → 二级菜单 → 执行
// ─────────────────────────────────────────────────────────────────────────────

fn run_schedule() -> anyhow::Result<()> {
    let path = config_path();

    if !path.exists() {
        anyhow::bail!(
            "配置文件未找到：{}，请先运行 corex schedule generate 生成配置",
            path.display()
        );
    }

    let configure = load_config(&path)?;

    if configure.pipelines.is_empty() {
        anyhow::bail!("配置文件中没有找到有效的流水线");
    }

    print_banner("Corex · 任务调度器");

    // ── 一级菜单：选择流水线 ────────────────────────────────────────────────────
    let mut labels: Vec<String> = Vec::new();
    for pipeline in configure.pipelines.iter() {
        let desc = pipeline.description.as_deref().unwrap_or(&pipeline.id);
        labels.push(format!(
            "{} {} ({} 步)",
            "▶".green().bold(),
            desc.bold(),
            pipeline.steps.len()
        ));
    }
    labels.push(format!("{} {}", "↩".dim(), "返回".dim()));

    let pipeline_idx = dialoguer::Select::with_theme(&theme())
        .with_prompt("选择流水线")
        .items(&labels)
        .default(0)
        .interact()
        .map_err(|e| anyhow::anyhow!("交互选择失败: {}", e))?;

    if pipeline_idx >= configure.pipelines.len() {
        return Ok(());
    }

    let pipeline = &configure.pipelines[pipeline_idx];

    // ── 二级菜单：选择执行方式 ─────────────────────────────────────────────────
    let mut mode_labels: Vec<String> = Vec::new();
    mode_labels.push(format!(
        "{} 全部执行（{} 步）",
        "▶".green().bold(),
        pipeline.steps.len()
    ));
    for (i, step) in pipeline.steps.iter().enumerate() {
        mode_labels.push(format!(
            "  {} 从第 {} 步开始：{}",
            "▸".cyan(),
            (i + 1).to_string().bold(),
            step_label(step, i)
        ));
    }
    mode_labels.push("  ✦ 选择要执行的步骤...".to_string());
    mode_labels.push(format!("{} {}", "↩".dim(), "返回".dim()));

    let mode_idx = dialoguer::Select::with_theme(&theme())
        .with_prompt("选择执行方式")
        .items(&mode_labels)
        .default(0)
        .interact()
        .map_err(|e| anyhow::anyhow!("交互选择失败: {}", e))?;

    let total = pipeline.steps.len();
    let mode = if mode_idx == 0 {
        ExecutionMode::All
    } else if mode_idx <= total {
        ExecutionMode::From(mode_idx - 1)
    } else if mode_idx == total + 1 {
        // 多选模式
        let step_labels: Vec<String> = pipeline
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}", step_label(s, i)))
            .collect();
        let selected = dialoguer::MultiSelect::with_theme(&theme())
            .with_prompt("勾选要执行的步骤（空格选择，回车确认）")
            .items(&step_labels)
            .interact()
            .map_err(|e| anyhow::anyhow!("交互选择失败: {}", e))?;
        if selected.is_empty() {
            println!("  {} 未选择任何步骤。", "×".red());
            return Ok(());
        }
        ExecutionMode::Selected(selected)
    } else {
        return Ok(());
    };

    run_pipeline(pipeline, &mode)
}

// ─────────────────────────────────────────────────────────────────────────────
// generate_config：交互式向导创建 pipeline 配置
// ─────────────────────────────────────────────────────────────────────────────

fn generate_config() -> anyhow::Result<()> {
    print_banner("Corex · 配置向导");
    let path = config_path();

    // 默认输出 .yaml
    let yaml_path = if path.extension().and_then(|e| e.to_str()) == Some("json") {
        path.with_extension("yaml")
    } else {
        path.clone()
    };

    if yaml_path.exists() {
        println!(
            "  {} 配置文件已存在：{}",
            "→".yellow(),
            yaml_path.display().to_string().dim()
        );
        let confirmed = dialoguer::Confirm::with_theme(&theme())
            .with_prompt("是否覆盖")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !confirmed {
            println!("  {} 已取消。", "×".red());
            return Ok(());
        }
    }

    // 询问流水线名称
    let pipeline_id: String = dialoguer::Input::with_theme(&theme())
        .with_prompt("流水线 ID（英文，无空格）")
        .default("default".to_string())
        .interact_text()
        .unwrap();

    let pipeline_desc: String = dialoguer::Input::with_theme(&theme())
        .with_prompt("流水线描述")
        .allow_empty(true)
        .interact_text()
        .unwrap();

    let task_types = [
        "  复制目录      copy",
        "  路径生成      generate path",
        "  UUID 生成     generate uuid",
        "  压缩打包      compression",
    ];
    println!();
    let selections = dialoguer::MultiSelect::with_theme(&theme())
        .with_prompt("选择要添加的步骤类型（按顺序）")
        .items(&task_types)
        .interact()
        .unwrap();

    if selections.is_empty() {
        println!("  {} 未选择任何步骤，已取消。", "×".red());
        return Ok(());
    }

    let mut steps: Vec<Step> = Vec::new();

    for &sel in &selections {
        match sel {
            0 => {
                print_section("复制步骤");
                let description: String = dialoguer::Input::with_theme(&theme())
                    .with_prompt("描述")
                    .default("复制任务".to_string())
                    .interact_text()
                    .unwrap();
                let from: String = {
                    let v = normalize_path(
                        &dialoguer::Input::<String>::with_theme(&theme())
                            .with_prompt("源路径（可用 $last_output 引用上一步输出）")
                            .interact_text()
                            .unwrap(),
                    );
                    if v != "$last_output" {
                        warn_if_missing(&v);
                    }
                    v
                };
                let to: String = normalize_path(
                    &dialoguer::Input::<String>::with_theme(&theme())
                        .with_prompt("目标路径")
                        .interact_text()
                        .unwrap(),
                );
                let empty: bool = dialoguer::Confirm::with_theme(&theme())
                    .with_prompt("清空目标目录")
                    .default(false)
                    .interact()
                    .unwrap_or(false);
                let ignores = prompt_list("忽略模式  逗号分隔，可留空");
                steps.push(Step::Copy(copy::schema::Args {
                    from,
                    to,
                    empty,
                    ignores,
                    id: None,
                    description: Some(description),
                }));
            }
            1 => {
                print_section("路径生成步骤");
                let description: String = dialoguer::Input::with_theme(&theme())
                    .with_prompt("描述")
                    .default("路径生成任务".to_string())
                    .interact_text()
                    .unwrap();
                let from: String = {
                    let v = normalize_path(
                        &dialoguer::Input::<String>::with_theme(&theme())
                            .with_prompt("源目录（可用 $last_output 引用上一步输出）")
                            .interact_text()
                            .unwrap(),
                    );
                    if v != "$last_output" {
                        warn_if_missing(&v);
                    }
                    v
                };
                let to: String = normalize_path(
                    &dialoguer::Input::<String>::with_theme(&theme())
                        .with_prompt("输出文件路径")
                        .interact_text()
                        .unwrap(),
                );
                let transform: String = dialoguer::Input::with_theme(&theme())
                    .with_prompt(
                        "转换规则  {{index}} {{filename}} {{extension}} {{path}} {{fullpath}}",
                    )
                    .interact_text()
                    .unwrap();
                let index: usize = dialoguer::Input::with_theme(&theme())
                    .with_prompt("起始索引")
                    .default(0usize)
                    .interact_text()
                    .unwrap();
                let separator: String = dialoguer::Input::with_theme(&theme())
                    .with_prompt("路径分隔符")
                    .default("/".to_string())
                    .interact_text()
                    .unwrap();
                let pad: bool = dialoguer::Confirm::with_theme(&theme())
                    .with_prompt("填充索引 (pad)")
                    .default(false)
                    .interact()
                    .unwrap_or(false);
                let ignores = prompt_list("忽略模式  逗号分隔，可留空");
                let uppercase = prompt_list("大写字段  如 extension,filename，可留空");
                steps.push(Step::GeneratePath(generate::schema::PathArgs {
                    from,
                    to,
                    transform,
                    index,
                    separator,
                    pad,
                    ignores,
                    uppercase,
                    id: None,
                    description: Some(description),
                }));
            }
            2 => {
                print_section("UUID 生成步骤");
                let description: String = dialoguer::Input::with_theme(&theme())
                    .with_prompt("描述")
                    .default("UUID 生成任务".to_string())
                    .interact_text()
                    .unwrap();
                let count: usize = dialoguer::Input::with_theme(&theme())
                    .with_prompt("生成数量")
                    .default(1usize)
                    .interact_text()
                    .unwrap();
                let uppercase: bool = dialoguer::Confirm::with_theme(&theme())
                    .with_prompt("大写输出")
                    .default(false)
                    .interact()
                    .unwrap_or(false);
                steps.push(Step::GenerateUuid(generate::schema::UuidArgs {
                    count,
                    uppercase,
                    id: None,
                    description: Some(description),
                }));
            }
            3 => {
                print_section("压缩步骤");
                let description: String = dialoguer::Input::with_theme(&theme())
                    .with_prompt("描述")
                    .default("压缩任务".to_string())
                    .interact_text()
                    .unwrap();
                let from: String = {
                    let v = normalize_path(
                        &dialoguer::Input::<String>::with_theme(&theme())
                            .with_prompt("源路径（可用 $last_output 引用上一步输出）")
                            .interact_text()
                            .unwrap(),
                    );
                    if v != "$last_output" {
                        warn_if_missing(&v);
                    }
                    v
                };
                let to: String = normalize_path(
                    &dialoguer::Input::<String>::with_theme(&theme())
                        .with_prompt("输出压缩包路径")
                        .interact_text()
                        .unwrap(),
                );
                steps.push(Step::Compression(compression::schema::Args {
                    from,
                    to,
                    id: None,
                    description: Some(description),
                }));
            }
            _ => {}
        }
    }

    let config = ScheduleConfig {
        pipelines: vec![Pipeline {
            id: pipeline_id,
            description: if pipeline_desc.is_empty() {
                None
            } else {
                Some(pipeline_desc)
            },
            steps,
        }],
    };

    let content =
        serde_yml::to_string(&config).map_err(|e| anyhow::anyhow!("序列化 YAML 失败: {}", e))?;
    let content = format!("# Corex 任务配置文件\n{}", content);

    let divider = "─".repeat(56);
    println!("\n{}", divider.clone().dim());
    println!(
        "  {} 写入路径：{}",
        "→".cyan(),
        yaml_path.display().to_string().bold()
    );
    println!("{}", divider.clone().dim());
    for line in content.lines() {
        println!("  {}", line.dim());
    }
    println!("{}", divider.dim());

    let confirmed = dialoguer::Confirm::with_theme(&theme())
        .with_prompt("确认写入")
        .default(true)
        .interact()
        .unwrap_or(false);

    if !confirmed {
        println!("  {} 已取消。", "×".red());
        return Ok(());
    }

    if let Some(parent) = yaml_path.parent() {
        fs::create_dir_all(parent).map_err(|e| anyhow::anyhow!("无法创建 .corex 目录: {}", e))?;
    }
    fs::write(&yaml_path, &content).map_err(|e| anyhow::anyhow!("写入配置文件失败: {}", e))?;
    println!(
        "\n  {} 配置已生成：{}",
        "✓".green().bold(),
        yaml_path.display().to_string().bold()
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// UI helpers
// ─────────────────────────────────────────────────────────────────────────────

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
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

fn print_section(title: &str) {
    println!("\n  {} {}", "▸".cyan(), title.to_string().bold());
    println!("  {}", "─".repeat(44).dim());
}

fn warn_if_missing(path_str: &str) {
    if !path_str.is_empty() && !Path::new(path_str).exists() {
        println!("    {} 路径不存在（将在运行时验证）", "⚠".yellow());
    }
}

/// 提示输入逗号分隔的列表，返回 Vec<String>
fn prompt_list(prompt: &str) -> Vec<String> {
    let input: String = dialoguer::Input::with_theme(&theme())
        .with_prompt(prompt)
        .allow_empty(true)
        .interact_text()
        .unwrap();
    if input.trim().is_empty() {
        vec![]
    } else {
        input.split(',').map(|s| s.trim().to_string()).collect()
    }
}

/// 规范化路径：将连续的反斜杠折叠为单个，UNC 路径（\\开头）保留前两个
fn normalize_path(input: &str) -> String {
    // 保留特殊变量标记
    if input == "$last_output" {
        return input.to_string();
    }
    let (prefix, rest) = if input.starts_with(r"\\") {
        (r"\\", &input[2..])
    } else {
        ("", input)
    };
    let mut result = String::with_capacity(input.len());
    let mut prev_slash = false;
    for ch in rest.chars() {
        if ch == '\\' {
            if !prev_slash {
                result.push(ch);
            }
            prev_slash = true;
        } else {
            result.push(ch);
            prev_slash = false;
        }
    }
    format!("{}{}", prefix, result)
}
