use crate::{copy, generate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Args {
    // #[serde(flatten)]
    pub copy: Vec<copy::controller::Args>,

    // #[serde(flatten)]
    pub generate: generate::controller::GenerateTask,
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
