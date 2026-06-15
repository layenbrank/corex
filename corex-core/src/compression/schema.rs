use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

/// compression 子命令
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 压缩打包为 ZIP
    Zip(ZipArgs),
    /// 解压 ZIP 文件
    Unzip(UnzipArgs),
}

// ─── ZIP 压缩 ────────────────────────────────────────────────────────────────

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ZipArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,

    #[arg(short, long)]
    pub to: String,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}

// ─── ZIP 解压缩 ──────────────────────────────────────────────────────────────

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct UnzipArgs {
    /// ZIP 文件路径
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,

    /// 解压输出目录
    #[arg(short, long)]
    pub to: String,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}
