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

    #[arg(short, long)]
    pub recursive: bool,

    #[arg(short, long)]
    pub separator: String,

    #[arg(long)]
    pub transform: String,

    #[arg(short, long, action = ArgAction::Append, value_delimiter = ',', help = "忽略模式，用逗号分隔")]
    pub ignores: Vec<String>,
}
