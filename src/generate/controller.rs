use crate::utils::verifier::Verifier;
use clap::{ArgAction, Parser};

#[derive(Debug, Parser)]
pub enum GenerateArgs {
    Path(PathArgs),
}

#[derive(Debug, Parser)]
pub struct PathArgs {
    #[arg(short, long, value_parser = Verifier::path)]
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

    #[arg(long, action = ArgAction::Append, value_delimiter = ',', help = "忽略模式，可多次使用或用逗号分隔")]
    pub ignores: Vec<String>,

    #[arg(long, action = ArgAction::Append, value_delimiter = ',', help = "将某个规则转换为大写，可多次使用或用逗号分隔")]
    pub uppercase: Vec<String>,
}
