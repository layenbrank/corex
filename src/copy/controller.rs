use crate::utils::verifier::Verifier;
use clap::{ArgAction, Parser};
// use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Parser)]
pub struct CopyArgs {
    #[arg(short, long, value_parser = Verifier::path, help = "源路径")]
    pub from: String,

    #[arg(short, long, value_parser = Verifier::path,help = "目标路径")]
    pub to: String,

    #[arg(short, long, action = ArgAction::SetTrue, help = "是否清空目标文件夹")]
    pub empty: bool,

    #[arg(short, long, action = ArgAction::Append, value_delimiter = ',', help = "忽略模式，用逗号分隔")]
    pub ignore: Vec<String>,
}
