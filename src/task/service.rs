use crate::task::controller::Args;
use crate::{copy, generate};
use config::{Config, File};
use dialoguer;
use dirs;
// use std::path::Path;
// use std::collections::HashMap;

#[derive(Debug, Clone)]
enum Class {
    Copy(usize), // 复制任务的索引
    GeneratePath(usize),
    // 生成路径任务的索引
    // 未来可以添加: GenerateFile(usize), GenerateTemplate(usize) 等
}

#[derive(Debug, Clone)]
struct Task {
    class: Class,
    description: String,
}

pub fn run() {
    let home_dir = dirs::home_dir().expect("无法获取用户目录");

    let path = home_dir
        .to_path_buf()
        .join(".corex")
        .join("corex-configure.json");

    if !path.exists() {
        eprintln!(
            "配置文件 corex-configure.json 未找到，请确保文件存在于当前目录。\n{}",
            path.display()
        );
        return;
    }
    let configure = Config::builder()
        .add_source(File::with_name(path.to_str().unwrap()))
        // .add_source(config::Environment::with_prefix("COREX"))
        .build()
        .expect("Failed to build configuration")
        // .try_deserialize::<HashMap<String, Args>>()
        .try_deserialize::<Args>()
        .expect("Failed to deserialize configuration");

    if configure.copy.is_empty() && configure.generate.path.is_empty() {
        eprintln!("配置文件中没有找到有效的任务");
        return;
    }

    let mut tasks: Vec<Task> = Vec::new();

    // 添加 copy 任务
    for (i, task) in configure.copy.iter().enumerate() {
        let description = task
            .description
            .clone()
            .unwrap_or_else(|| format!("复制任务 {}", i + 1));

        tasks.push(Task {
            class: Class::Copy(i),
            description,
        });
    }

    // 添加 generate.path 任务
    for (i, task) in configure.generate.path.iter().enumerate() {
        let description = task
            .description
            .clone()
            .unwrap_or_else(|| format!("生成路径任务 {}", i + 1));

        tasks.push(Task {
            class: Class::GeneratePath(i),
            description,
        });
    }

    // 未来可以在这里添加其他 generate 类型的任务
    // 例如: configure.generate.file, configure.generate.template 等

    // 让用户选择任务
    let choice = dialoguer::Select::new()
        .with_prompt("请选择要执行的任务")
        .items(
            &tasks
                .iter()
                .map(|t| t.description.as_str())
                .collect::<Vec<_>>(),
        )
        .interact()
        .unwrap();

    let task = &tasks[choice];

    // 根据任务类型执行相应的任务
    match &task.class {
        Class::Copy(index) => {
            let task = &configure.copy[*index];
            println!("执行复制任务: {:#?}", task);
            copy::service::run(task);
        }
        Class::GeneratePath(index) => {
            let task = &configure.generate.path[*index];
            println!("执行生成路径任务: {:#?}", task);
            generate::service::run(&generate::controller::Args::Path(task.clone()));
        } // 未来可以在这里添加其他 generate 类型的匹配分支
          // Class::GenerateFile(index) => { ... }
          // Class::GenerateTemplate(index) => { ... }
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
