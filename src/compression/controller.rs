use clap::Parser;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::utils::verifier;

#[derive(Parser, Debug, Clone, Deserialize, Serialize)]
pub struct Args {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,

    #[arg(short, long)]
    pub to: String,

    #[arg(help = "任务描述")]
    pub description: Option<String>,

    #[arg(help = "任务ID")]
    pub id: Option<String>,
}

#[derive(Error, Debug)]
pub enum Exception {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error), // from → ? 自动转换

    #[error("无效的构建产物: {0}")]
    InvalidInput(String),

    #[error("压缩错误: {0}")]
    Zip(#[from] zip::result::ZipError),
}
