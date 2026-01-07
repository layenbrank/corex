use crate::utils::verifier::Verifier;
use clap::{ArgAction, Parser};

#[derive(Debug, Clone, Parser)]
pub struct Args {
    #[arg(short, long, value_parser = Verifier::path,help = "目标路径")]
    // 源路径（根目录）
    pub source: String,

    // 要删除的目标名称（仅名称，不包含路径前缀）
    #[arg(short, long, help = "要删除的目标名称（不含前缀路径）")]
    pub target: String,

    #[arg(short, long, action = ArgAction::Append, default_value_t = false, help = "是否递归删除(默认false)")]
    pub recursive: bool,
}
