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
