use std::{
    fs,
    path::{Path, PathBuf},
};

use config::{Config as Configure, File};
use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;

use uuid::Uuid;

use crate::schedule::controller::{Args, Pipeline, ScheduleConfig, Step};
use crate::schedule::pipeline::Context;
use crate::{compression, copy, generate};

pub fn run(args: &Args) {
    match args {
        Args::Run => run_schedule(),
        Args::Generate => generate_config(),
    }
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .expect("无法获取用户目录")
        .join(".corex")
        .join("corex-configure.json")
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline 执行引擎
// ─────────────────────────────────────────────────────────────────────────────

/// 执行整条 pipeline：按顺序运行每个 step，共享 Context
fn run_pipeline(pipeline: &Pipeline) {
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

    for (i, step) in pipeline.steps.iter().enumerate() {
        let step_label = step_label(step, i);
        println!("  {} [{}] {}", "▸".cyan(), (i + 1).to_string().bold(), step_label);

        if let Err(e) = execute_step(step, &mut ctx) {
            eprintln!(
                "  {} 步骤 {} 失败: {}",
                "×".red(),
                (i + 1).to_string().bold(),
                e
            );
            return;
        }
    }

    println!(
        "\n  {} 流水线执行完成（共 {} 步）",
        "✓".green().bold(),
        pipeline.steps.len()
    );
}

/// 执行单个步骤并更新 Context
fn execute_step(step: &Step, ctx: &mut Context) -> Result<(), Box<dyn std::error::Error>> {
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
        Step::Compression(args) => {
            compression::service::execute(args, ctx)
                .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
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
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// run_schedule：加载配置 → 交互选择 → 执行
// ─────────────────────────────────────────────────────────────────────────────

fn run_schedule() {
    let path = config_path();

    if !path.exists() {
        eprintln!(
            "  {} 配置文件未找到：{}\n  请先运行 {} 生成配置",
            "×".red(),
            path.display().to_string().dim(),
            "corex schedule generate".bold()
        );
        return;
    }

    let configure = Configure::builder()
        .add_source(File::with_name(path.to_str().unwrap()))
        .build()
        .expect("Failed to build configuration")
        .try_deserialize::<ScheduleConfig>()
        .expect("Failed to deserialize configuration");

    if configure.pipelines.is_empty() {
        eprintln!("  {} 配置文件中没有找到有效的流水线", "×".red());
        return;
    }

    print_banner("Corex · 任务调度器");

    // 构建选项列表：每条流水线 + 各流水线内的单独步骤
    let mut choices: Vec<ScheduleChoice> = Vec::new();
    let mut labels: Vec<String> = Vec::new();

    for pipeline in &configure.pipelines {
        let desc = pipeline
            .description
            .as_deref()
            .unwrap_or(&pipeline.id);
        choices.push(ScheduleChoice::Pipeline(pipeline.clone()));
        labels.push(format!(
            "{} {} ({} 步)",
            "▶".green().bold(),
            desc.bold(),
            pipeline.steps.len()
        ));

        // 展开各步骤为可选项
        for (i, step) in pipeline.steps.iter().enumerate() {
            choices.push(ScheduleChoice::Step {
                pipeline: pipeline.clone(),
                step_index: i,
            });
            labels.push(format!("    {}", step_label(step, i)));
        }
    }

    let selected = dialoguer::Select::with_theme(&theme())
        .with_prompt("选择要执行的任务")
        .items(&labels)
        .default(0)
        .interact()
        .unwrap();

    match &choices[selected] {
        ScheduleChoice::Pipeline(pipeline) => run_pipeline(pipeline),
        ScheduleChoice::Step {
            pipeline,
            step_index,
        } => {
            let step = &pipeline.steps[*step_index];
            println!(
                "\n  {} 执行：{}\n",
                "▶".green().bold(),
                step_label(step, *step_index).bold()
            );
            let mut ctx = Context::new();
            if let Err(e) = execute_step(step, &mut ctx) {
                eprintln!("  {} 步骤执行失败: {}", "×".red(), e);
            }
        }
    }
}

#[derive(Debug, Clone)]
enum ScheduleChoice {
    Pipeline(Pipeline),
    Step {
        pipeline: Pipeline,
        step_index: usize,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// generate_config：交互式向导创建 pipeline 配置
// ─────────────────────────────────────────────────────────────────────────────

fn generate_config() {
    print_banner("Corex · 配置向导");
    let path = config_path();

    if path.exists() {
        println!(
            "  {} 配置文件已存在：{}",
            "→".yellow(),
            path.display().to_string().dim()
        );
        let confirmed = dialoguer::Confirm::with_theme(&theme())
            .with_prompt("是否覆盖")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !confirmed {
            println!("  {} 已取消。", "×".red());
            return;
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
        return;
    }

    let mut steps: Vec<serde_json::Value> = Vec::new();

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
                    if v != "$last_output" { warn_if_missing(&v); }
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
                steps.push(serde_json::json!({
                    "type": "copy",
                    "id": Uuid::new_v4().to_string(),
                    "description": description,
                    "from": from,
                    "to": to,
                    "empty": empty,
                    "ignores": ignores
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
                    if v != "$last_output" { warn_if_missing(&v); }
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
                steps.push(serde_json::json!({
                    "type": "generate-path",
                    "id": Uuid::new_v4().to_string(),
                    "description": description,
                    "from": from,
                    "to": to,
                    "transform": transform,
                    "index": index,
                    "separator": separator,
                    "pad": pad,
                    "ignores": ignores,
                    "uppercase": uppercase
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
                steps.push(serde_json::json!({
                    "type": "generate-uuid",
                    "id": Uuid::new_v4().to_string(),
                    "description": description,
                    "count": count,
                    "uppercase": uppercase
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
                    if v != "$last_output" { warn_if_missing(&v); }
                    v
                };
                let to: String = normalize_path(
                    &dialoguer::Input::<String>::with_theme(&theme())
                        .with_prompt("输出压缩包路径")
                        .interact_text()
                        .unwrap(),
                );
                steps.push(serde_json::json!({
                    "type": "compression",
                    "id": Uuid::new_v4().to_string(),
                    "description": description,
                    "from": from,
                    "to": to
                }));
            }
            _ => {}
        }
    }

    let config = serde_json::json!({
        "pipelines": [
            {
                "id": pipeline_id,
                "description": if pipeline_desc.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(pipeline_desc) },
                "steps": steps
            }
        ]
    });
    let content = serde_json::to_string_pretty(&config).expect("序列化失败");

    let divider = "─".repeat(56);
    println!("\n{}", divider.clone().dim());
    println!(
        "  {} 写入路径：{}",
        "→".cyan(),
        path.display().to_string().bold()
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
        return;
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("无法创建 .corex 目录");
    }
    fs::write(&path, &content).expect("写入配置文件失败");
    println!(
        "\n  {} 配置已生成：{}",
        "✓".green().bold(),
        path.display().to_string().bold()
    );
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
