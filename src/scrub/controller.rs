use crate::utils::verifier::Verifier;
use clap::{ArgAction, Parser};

#[derive(Debug, Clone, Parser)]
pub struct Args {
    // #[arg(short, long, value_parser = Verifier::path)]
    // pub source: String,
    #[arg(short, long, value_parser = Verifier::path,help = "目标路径")]
    pub target: String,

    #[arg(short, long, action = ArgAction::Append, default_value_t = false, help = "是否递归删除(默认false)")]
    pub recursive: bool,
}
// directory
// file
