use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

/// shade 图片处理参数
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct Args {
    /// 输入图片路径或目录
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,

    /// 输出路径（文件或与 from 同目录时自动改名）
    #[arg(short, long)]
    pub to: String,

    /// 输出格式：png / jpg / webp / bmp（留空则按 to 扩展名推断）
    #[arg(short = 'o', long)]
    pub format: Option<String>,

    /// 输出质量 1-100（仅对 jpg 有效；webp/png 始终无损）
    #[arg(short, long, default_value = "100")]
    pub quality: u8,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}
