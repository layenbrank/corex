use clap::{ArgAction, Parser};
use serde::Deserialize;

#[derive(Debug, Clone, Parser, Deserialize)]
pub struct CopyArgs {
    #[arg(short, long)]
    pub from: String,

    #[arg(short, long)]
    pub to: String,

    #[arg(short, long, action = ArgAction::Append, value_delimiter = ',', help = "忽略模式，用逗号分隔")]
    pub ignores: Vec<String>,
}
