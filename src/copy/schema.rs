use clap::{ArgAction, Parser};
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
pub struct Args {
    #[arg(short, long, value_parser = verifier::path, help = "源路径")]
    pub from: String,

    #[arg(short, long, value_parser = verifier::path, help = "目标路径")]
    pub to: String,

    #[arg(short, long, action = ArgAction::Append, default_value_t = true, hide_default_value = false,
        hide_possible_values = true, help = "是否清空目标文件夹"
	)]
    pub empty: bool,

    #[arg(short, long, action = ArgAction::Append, value_delimiter = ',', help = "忽略模式，用逗号分隔"
	)]
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub ignores: Vec<String>,

    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}
