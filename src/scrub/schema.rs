use clap::{ArgAction, Parser};
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
pub struct Args {
    #[arg(short, long, value_parser = verifier::path, help = "目标路径")]
    // 源路径（根目录）
    pub source: String,

    // 要删除的目标名称（仅名称，不包含路径前缀）
    #[arg(short, long, help = "要删除的目标名称（不含前缀路径）")]
    pub target: String,

    #[arg(short, long, action = ArgAction::Append, default_value_t = false, help = "递归删除")]
    pub recursive: bool,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}
