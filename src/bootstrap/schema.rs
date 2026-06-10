use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
pub enum Args {
    Env,
    Inspect,
    Force,
}
// #[derive(Debug, Clone, Parser, Serialize, Deserialize)]
// pub struct Args {
//     #[arg(long, action = ArgAction::SetTrue, help = "设置环境变量")]
//     pub env: bool,

//     #[arg(long, action = ArgAction::SetTrue, help = "检查环境变量")]
//     pub inspect: bool,

//     #[arg(long, action = ArgAction::SetTrue, help = "强制设置环境变量")]
//     pub force: bool,
// }
