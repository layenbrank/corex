use clap::{ArgAction, Parser, ValueEnum};
use serde::{Deserialize, Serialize};

/// stdout 捕获模式
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    /// 解析 stdout 最后一行 JSON（须含 path + data）
    #[default]
    Json,
    /// 原样保留 stdout 文本
    Text,
    /// 忽略 stdout
    None,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 运行外部脚本（.ps1 / .bat / .exe）
    Run(RunArgs),
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct RunArgs {
    /// 脚本或可执行文件路径
    #[arg(short, long)]
    pub script: String,

    /// 传给脚本的参数
    #[arg(action = ArgAction::Append)]
    #[serde(default)]
    pub args: Vec<String>,

    /// 子进程工作目录
    #[arg(short, long)]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cwd: Option<String>,

    /// stdout 捕获模式
    #[arg(long, default_value = "json")]
    #[serde(default)]
    pub capture: CaptureMode,
}
