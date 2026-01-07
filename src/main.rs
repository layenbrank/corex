use clap::Parser;
use core::{bootstrap, copy, generate, schedule, scrub};

#[derive(Debug, Parser)]
pub enum Commands {
    Copy(copy::controller::Args),
    Scrub(scrub::controller::Args),

    #[command(subcommand)]
    Generate(generate::controller::Args),

    #[command(subcommand)]
    Bootstrap(bootstrap::controller::Args),

    // #[command(subcommand)]
    Schedule,
}

#[derive(Debug, Parser)]
#[command(
	author = "layen <15638470820@163.com>",
	version = env!("CARGO_PKG_VERSION"),
	about = "Corex Tools",
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[tokio::main]
async fn main() {
    match Args::parse().command {
        Commands::Schedule => schedule::service::run(),
        Commands::Copy(args) => copy::service::run(&args),
        Commands::Scrub(args) => scrub::service::run(&args),
        Commands::Generate(args) => generate::service::run(&args),
        Commands::Bootstrap(args) => bootstrap::service::run(&args),
    }
}

// use config::Config;
// use serde::Deserialize;
// use std::{collections::HashMap};
// let settings = Config::builder()
//     .add_source(config::File::with_name("config.json"))
//     .add_source(config::Environment::with_prefix("config"))
//     .build()
//     .expect("配置文件获取失败");
//
// let deserialize = settings
//     .try_deserialize::<HashMap<String, copy::controller::CopyArgs>>()
//     .expect("转换失败");
// println!("deserialize {:?}", deserialize);

// 获取当前工作目录（调用命令的目录）
//     let current_dir = env::current_dir()
//         .expect("无法获取当前工作目录");
//     println!("当前工作目录: {:?}", current_dir);

// 获取可执行文件所在路径
//     let exe_path = env::current_exe()
//         .expect("无法获取可执行文件路径");
//     println!("可执行文件路径: {:?}", exe_path);

// 获取可执行文件所在目录
//     if let Some(exe_dir) = exe_path.parent() {
//         println!("可执行文件目录: {:?}", exe_dir);
//     }

// 获取特定环境变量
// if let Ok(path) = env::var("PATH") {
//     println!("PATH: {}", path);
// }

// 获取所有环境变量
// for (key, value) in env::vars() {
//     println!("{}: {}", key, value);
// }

// 获取用户主目录
// if let Ok(home) = env::var("USERPROFILE") { // Windows
//     println!("用户主目录: {}", home);
// } else if let Ok(home) = env::var("HOME") { // Unix/Linux
//     println!("用户主目录: {}", home);
// }

// 跨平台获取主目录 添加依赖 dirs
// if let Some(home_dir) = dirs::home_dir() {
//     println!("主目录: {:?}", home_dir);
// }

// 获取当前工作目录
// pub fn current_dir() -> PathBuf {
//     env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
// }

// 获取可执行文件目录
// pub fn exe_dir() -> Option<PathBuf> {
//     env::current_exe()
//         .ok()
//         .and_then(|path| path.parent().map(|p| p.to_path_buf()))
// }

// 获取环境变量
// pub fn get_var(key: &str) -> Option<String> {
//     env::var(key).ok()
// }

// 获取用户主目录
// pub fn home_dir() -> Option<PathBuf> {
//     env::var("USERPROFILE") // Windows
//         .or_else(|_| env::var("HOME")) // Unix/Linux
//         .ok()
//         .map(PathBuf::from)
// }

// 解析相对路径为绝对路径
// pub fn resolve_path(path: &str) -> PathBuf {
//     let path = PathBuf::from(path);
//     if path.is_absolute() {
//         path
//     } else {
//         Self::current_dir().join(path)
//     }
// }

// use std::env;

// fn print_common_env_vars() {
// Windows 特有
//     if let Ok(val) = env::var("USERPROFILE") {
//         println!("USERPROFILE: {}", val);
//     }
//     if let Ok(val) = env::var("APPDATA") {
//         println!("APPDATA: {}", val);
//     }
//     if let Ok(val) = env::var("LOCALAPPDATA") {
//         println!("LOCALAPPDATA: {}", val);
//     }

// 通用
//     if let Ok(val) = env::var("PATH") {
//         println!("PATH: {}", val);
//     }
//     if let Ok(val) = env::var("TEMP") {
//         println!("TEMP: {}", val);
//     }
//     if let Ok(val) = env::var("USERNAME") {
//         println!("USERNAME: {}", val);
//     }
// }
