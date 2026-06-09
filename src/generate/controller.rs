use clap::{ArgAction, Parser};
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSchedule {
    pub path: Vec<PathArgs>,
    #[serde(default)]
    pub uuid: Vec<UuidArgs>,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    Path(PathArgs),
    Uuid(UuidArgs),
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct UuidArgs {
    #[arg(short, long, default_value = "1", help = "生成 UUID 的数量")]
    pub count: usize,

    #[arg(long, action = ArgAction::SetTrue, help = "以大写形式输出")]
    pub uppercase: bool,

    #[arg(help = "任务ID")]
    pub id: Option<String>,

    #[arg(help = "任务描述")]
    pub description: Option<String>,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct PathArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,

    #[arg(short, long)]
    pub to: String,

    // #[arg(short, long)]
    // pub recursive: bool,
    #[arg(long, help = "转换规则")]
    pub transform: String,

    #[arg(long, help = "起始索引")]
    pub index: usize,

    #[arg(long, help = "路径分隔符")]
    pub separator: String,

    #[arg(long, action = ArgAction::SetTrue, help = "填充索引")]
    pub pad: bool,

    #[arg(long, action = ArgAction::Append, value_delimiter = ',', help = "忽略模式，可多次使用或用逗号分隔"
	)]
    pub ignores: Vec<String>,

    #[arg(long, action = ArgAction::Append, value_delimiter = ',', help = "将某个规则转换为大写，可多次使用或用逗号分隔"
	)]
    pub uppercase: Vec<String>,

    #[arg(help = "任务ID")]
    pub id: Option<String>,

    #[arg(help = "任务描述")]
    pub description: Option<String>,
}
