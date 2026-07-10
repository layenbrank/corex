use clap::{ArgAction, Parser, ValueEnum};
use serde::{Deserialize, Serialize};

use super::emit::OutputFormat;

/// 终端颜色策略
#[derive(Debug, Clone, Copy, Default, ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

/// 全局运行时选项（clap global args）
#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
pub struct RuntimeOpts {
    /// 输出格式：human（默认）| json
    #[arg(long, global = true, default_value = "human")]
    pub format: OutputFormat,
    /// 仅输出结果，抑制进度与 banner
    #[arg(short, long, global = true)]
    pub quiet: bool,
    /// 启用 tracing DEBUG（可重复 -vv）
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
    /// 颜色：auto | always | never
    #[arg(long, global = true, default_value = "auto")]
    pub color: ColorChoice,
}
