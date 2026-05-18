use crate::{compression, copy, generate};
use clap::Parser;
use serde::{Deserialize, Serialize};

/// schedule 子命令
#[derive(Debug, Parser)]
pub enum Args {
    /// 交互式选择并执行配置任务
    Run,
    /// 在 ~/.corex/ 生成配置文件模板
    Generate,
}

/// ~/.corex/corex-configure.json 反序列化结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    #[serde(default)]
    pub copy: Vec<copy::controller::Args>,

    pub generate: generate::controller::GenerateSchedule,

    #[serde(default)]
    pub compression: Vec<compression::controller::Args>,
}

// #[derive(Debug, Parser)]
// pub enum Args {
// 	Env,
// }

// #[derive(Debug)]
// pub struct CorexConfig {
//     pub copy: Option<Vec<HashMap<String, CopyTask>>>,
//     pub generate: Option<GenerateConfig>,
// }

// #[derive(Debug)]
// pub struct CopyTask {
//     pub from: String,
//     pub to: String,
//     pub ignores: Option<Vec<String>>,
//     pub empty: Option<bool>,
// }

// #[derive(Debug)]
// pub struct GenerateConfig {
//     pub path: Option<Vec<HashMap<String, PathTask>>>,
// }

// #[derive(Debug)]
// pub struct PathTask {
//     pub from: String,
//     pub to: String,
//     pub ignores: Option<Vec<String>>,
//     pub index: Option<u32>,
//     pub separator: Option<String>,
//     pub uppercase: Option<Vec<String>>,
//     pub transform: Option<String>,
//

// pub fn run(task: Args) {
// 	match task {
// 		Args::Env => {
// 			// 这里可以添加环境变量的处理逻辑
// 			println!("Running Env task...");
// 		}
// 	}
// }
//
// #[derive(Debug, Clone, Parser, Serialize, Deserialize)]
// pub struct BaseTask {
// 	pub from: String,
// 	pub to: String,
// 	pub ignores: Option<Vec<String>>,
// }
//
// #[derive(Serialize, Deserialize, Debug)]
// pub struct CopyTask {
// 	#[serde(flatten)] // 关键：将基础字段平铺到当前结构
// 	pub base: BaseTask,
// 	pub empty: Option<bool>,
// }
//
// impl CopyTask {
// 	pub fn new(
// 		from: String,
// 		to: String,
// 		ignores: Option<Vec<String>>,
// 		empty: Option<bool>,
// 	) -> Self {
// 		Self {
// 			base: BaseTask { from, to, ignores },
// 			empty,
// 		}
// 	}
// }
//
// #[derive(Serialize, Deserialize, Debug)]
// pub struct PathTask {
// 	#[serde(flatten)] // 关键：将基础字段平铺到当前结构
// 	pub base: BaseTask,
// 	pub index: Option<u32>,
// 	pub separator: Option<String>,
// 	pub uppercase: Option<Vec<String>>,
// 	pub transform: Option<String>,
// }
//
// #[derive(Debug)]
// pub struct GenerateConfig {
// 	pub path: Option<Vec<HashMap<String, PathTask>>>,
// }
//
// #[derive(Debug)]
// pub struct CorexConfig {
// 	pub copy: Option<Vec<HashMap<String, CopyTask>>>,
// 	pub generate: Option<GenerateConfig>,
// }
