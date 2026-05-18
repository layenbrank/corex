use crate::schedule::controller::{Args, ScheduleConfig};
use crate::{compression, copy, generate};
use config::{Config as Configure, File};
use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;
use dirs;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
enum Segment {
    Copy(usize),
    GeneratePath(usize),
    GenerateUuid(usize),
    Compression(usize),
}

#[derive(Debug, Clone)]
struct Schedule {
    segment: Segment,
    description: String,
}

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

    let task_types = [
        "  复制目录      copy",
        "  路径生成      generate path",
        "  UUID 生成     generate uuid",
        "  压缩打包      compression",
    ];
    println!();
    let selections = dialoguer::MultiSelect::with_theme(&theme())
        .with_prompt("选择任务类型")
        .items(&task_types)
        .interact()
        .unwrap();

    if selections.is_empty() {
        println!("  {} 未选择任何类型，已取消。", "×".red());
        return;
    }

    let mut copy_tasks: Vec<serde_json::Value> = Vec::new();
    let mut path_tasks: Vec<serde_json::Value> = Vec::new();
    let mut uuid_tasks: Vec<serde_json::Value> = Vec::new();
    let mut compression_tasks: Vec<serde_json::Value> = Vec::new();

    for &sel in &selections {
        match sel {
            0 => {
                let n: usize = dialoguer::Input::with_theme(&theme())
                    .with_prompt("复制任务数量")
                    .default(1)
                    .interact_text()
                    .unwrap();
                for i in 0..n {
                    print_section(&format!("复制任务 {}/{}", i + 1, n));
                    let description: String = dialoguer::Input::with_theme(&theme())
                        .with_prompt("描述")
                        .default(format!("复制任务 {}", i + 1))
                        .interact_text()
                        .unwrap();
                    let from: String = {
                        let v = normalize_path(
                            &dialoguer::Input::<String>::with_theme(&theme())
                                .with_prompt("源路径")
                                .interact_text()
                                .unwrap(),
                        );
                        warn_if_missing(&v);
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
                    copy_tasks.push(serde_json::json!({
                        "id": Uuid::new_v4().to_string(),
                        "description": description,
                        "from": from,
                        "to": to,
                        "empty": empty,
                        "ignores": ignores
                    }));
                }
            }
            1 => {
                let n: usize = dialoguer::Input::with_theme(&theme())
                    .with_prompt("路径生成任务数量")
                    .default(1)
                    .interact_text()
                    .unwrap();
                for i in 0..n {
                    print_section(&format!("路径生成任务 {}/{}", i + 1, n));
                    let description: String = dialoguer::Input::with_theme(&theme())
                        .with_prompt("描述")
                        .default(format!("路径生成任务 {}", i + 1))
                        .interact_text()
                        .unwrap();
                    let from: String = {
                        let v = normalize_path(
                            &dialoguer::Input::<String>::with_theme(&theme())
                                .with_prompt("源目录")
                                .interact_text()
                                .unwrap(),
                        );
                        warn_if_missing(&v);
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
                    path_tasks.push(serde_json::json!({
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
            }
            2 => {
                let n: usize = dialoguer::Input::with_theme(&theme())
                    .with_prompt("UUID 生成任务数量")
                    .default(1)
                    .interact_text()
                    .unwrap();
                for i in 0..n {
                    print_section(&format!("UUID 生成任务 {}/{}", i + 1, n));
                    let description: String = dialoguer::Input::with_theme(&theme())
                        .with_prompt("描述")
                        .default(format!("UUID 生成任务 {}", i + 1))
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
                    uuid_tasks.push(serde_json::json!({
                        "id": Uuid::new_v4().to_string(),
                        "description": description,
                        "count": count,
                        "uppercase": uppercase
                    }));
                }
            }
            3 => {
                let n: usize = dialoguer::Input::with_theme(&theme())
                    .with_prompt("压缩任务数量")
                    .default(1)
                    .interact_text()
                    .unwrap();
                for i in 0..n {
                    print_section(&format!("压缩任务 {}/{}", i + 1, n));
                    let description: String = dialoguer::Input::with_theme(&theme())
                        .with_prompt("描述")
                        .default(format!("压缩任务 {}", i + 1))
                        .interact_text()
                        .unwrap();
                    let from: String = {
                        let v = normalize_path(
                            &dialoguer::Input::<String>::with_theme(&theme())
                                .with_prompt("源路径")
                                .interact_text()
                                .unwrap(),
                        );
                        warn_if_missing(&v);
                        v
                    };
                    let to: String = normalize_path(
                        &dialoguer::Input::<String>::with_theme(&theme())
                            .with_prompt("输出压缩包路径")
                            .interact_text()
                            .unwrap(),
                    );
                    compression_tasks.push(serde_json::json!({
                        "id": Uuid::new_v4().to_string(),
                        "description": description,
                        "from": from,
                        "to": to
                    }));
                }
            }
            _ => {}
        }
    }

    let config = serde_json::json!({
        "copy": copy_tasks,
        "generate": {
            "path": path_tasks,
            "uuid": uuid_tasks
        },
        "compression": compression_tasks
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

/// 提示输入逗号分隔的列表，返回 Vec<String>，空输入返回空 Vec
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

    if configure.copy.is_empty()
        && configure.generate.path.is_empty()
        && configure.generate.uuid.is_empty()
        && configure.compression.is_empty()
    {
        eprintln!("  {} 配置文件中没有找到有效的任务", "×".red());
        return;
    }

    let mut schedules: Vec<Schedule> = Vec::new();
    let mut labels: Vec<String> = Vec::new();

    for (i, s) in configure.copy.iter().enumerate() {
        let desc = s
            .description
            .clone()
            .unwrap_or_else(|| format!("复制任务 {}", i + 1));
        schedules.push(Schedule {
            segment: Segment::Copy(i),
            description: desc.clone(),
        });
        labels.push(format!("{} {}", "[复制]".cyan().bold(), desc));
    }
    for (i, s) in configure.generate.path.iter().enumerate() {
        let desc = s
            .description
            .clone()
            .unwrap_or_else(|| format!("路径生成 {}", i + 1));
        schedules.push(Schedule {
            segment: Segment::GeneratePath(i),
            description: desc.clone(),
        });
        labels.push(format!("{} {}", "[路径]".green().bold(), desc));
    }
    for (i, s) in configure.generate.uuid.iter().enumerate() {
        let desc = s
            .description
            .clone()
            .unwrap_or_else(|| format!("UUID 生成 {}", i + 1));
        schedules.push(Schedule {
            segment: Segment::GenerateUuid(i),
            description: desc.clone(),
        });
        labels.push(format!("{} {}", "[UUID]".magenta().bold(), desc));
    }
    for (i, s) in configure.compression.iter().enumerate() {
        let desc = s
            .description
            .clone()
            .unwrap_or_else(|| format!("压缩任务 {}", i + 1));
        schedules.push(Schedule {
            segment: Segment::Compression(i),
            description: desc.clone(),
        });
        labels.push(format!("{} {}", "[压缩]".yellow().bold(), desc));
    }

    print_banner("Corex · 任务调度器");

    let choice = dialoguer::Select::with_theme(&theme())
        .with_prompt("选择要执行的任务")
        .items(&labels)
        .default(0)
        .interact()
        .unwrap();

    let schedule = &schedules[choice];
    println!(
        "\n  {} 执行：{}\n",
        "▶".green().bold(),
        schedule.description.as_str().bold()
    );

    match &schedule.segment {
        Segment::Copy(index) => copy::service::run(&configure.copy[*index]),
        Segment::GeneratePath(index) => generate::service::run(&generate::controller::Args::Path(
            configure.generate.path[*index].clone(),
        )),
        Segment::GenerateUuid(index) => {
            generate::service::uuid_task(&configure.generate.uuid[*index])
        }
        Segment::Compression(index) => compression::service::run(&configure.compression[*index]),
    }
}

// let args = Args {
//     copy: vec![copy::controller::CopyTask {
//         id: "copy_task_1".to_string(),
//         description: "复制任务示例".to_string(),
//         from: String::from("src"),
//         to: String::from("dest"),
//         empty: true,
//         ignores: vec![String::from("*.tmp"), String::from("node_modules/")],
//     }],
//     generate: generate::controller::GenerateTask {
//         path: vec![generate::controller::PathTask {
//             id: "generate_task_1".to_string(),
//             description: "生成任务示例".to_string(),
//             from: String::from("template/{index}/file.txt"),
//             to: String::from("output/file-{index}.txt"),
//             transform: String::from("{index}"),
//             index: 1,
//             separator: String::from("-"),
//             pad: true,
//             ignores: vec![String::from("*.log")],
//             uppercase: vec![String::from("{index}")],
//         }],
//     },
// };
// println!("args {:#?}", args);

// ========== example 1 ========== //
//  配置文件加载和保存

// use std::fs;
// use std::path::Path;

// impl CorexConfig {
//     /// 从文件加载配置
//     pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
//         let content = fs::read_to_string(path)?;
//         let config: CorexConfig = serde_json::from_str(&content)?;
//         Ok(config)
//     }

//     /// 保存配置到文件
//     pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
//         let content = serde_json::to_string_pretty(self)?;
//         fs::write(path, content)?;
//         Ok(())
//     }

//     /// 创建默认配置
//     pub fn default() -> Self {
//         CorexConfig {
//             copy: None,
//             generate: None,
//         }
//     }
// }

// ========== example 2 ========== //
// 在主程序中使用
// mod config;

// use config::CorexConfig;
// use std::env;

// fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // 获取配置文件路径
//     let config_path = env::args().nth(1)
//         .unwrap_or_else(|| "Corex.task.json".to_string());

//     // 加载配置
//     let config = CorexConfig::load_from_file(&config_path)?;

//     println!("加载配置成功: {:#?}", config);

//     // 执行任务
//     execute_tasks(&config)?;

//     Ok(())
// }

// fn execute_tasks(config: &CorexConfig) -> Result<(), Box<dyn std::error::Error>> {
//     // 执行复制任务
//     if let Some(copy_tasks) = &config.copy {
//         for task_map in copy_tasks {
//             for (task_name, task) in task_map {
//                 println!("执行复制任务: {}", task_name);
//                 execute_copy_task(task)?;
//             }
//         }
//     }

//     // 执行生成任务
//     if let Some(generate_config) = &config.generate {
//         if let Some(path_tasks) = &generate_config.path {
//             for task_map in path_tasks {
//                 for (task_name, task) in task_map {
//                     println!("执行路径生成任务: {}", task_name);
//                     execute_path_task(task)?;
//                 }
//             }
//         }
//     }

//     Ok(())
// }

// fn execute_copy_task(task: &config::CopyTask) -> Result<(), Box<dyn std::error::Error>> {
//     println!("从 {} 复制到 {}", task.from, task.to);
//     // 在这里实现具体的复制逻辑
//     Ok(())
// }

// fn execute_path_task(task: &config::PathTask) -> Result<(), Box<dyn std::error::Error>> {
//     println!("生成路径任务: {} -> {}", task.from, task.to);
//     // 在这里实现具体的路径生成逻辑
//     Ok(())
// }

// ========== example 3 ========== //
// 更好的错误处理（可选）
// use std::fmt;

// #[derive(Debug)]
// pub enum CorexError {
//     ConfigNotFound(String),
//     InvalidConfig(String),
//     IoError(std::io::Error),
//     JsonError(serde_json::Error),
// }

// impl fmt::Display for CorexError {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         match self {
//             CorexError::ConfigNotFound(path) => write!(f, "配置文件未找到: {}", path),
//             CorexError::InvalidConfig(msg) => write!(f, "配置文件无效: {}", msg),
//             CorexError::IoError(err) => write!(f, "IO错误: {}", err),
//             CorexError::JsonError(err) => write!(f, "JSON解析错误: {}", err),
//         }
//     }
// }

// impl std::error::Error for CorexError {}

// impl From<std::io::Error> for CorexError {
//     fn from(err: std::io::Error) -> Self {
//         CorexError::IoError(err)
//     }
// }

// impl From<serde_json::Error> for CorexError {
//     fn from(err: serde_json::Error) -> Self {
//         CorexError::JsonError(err)
//     }
// }
