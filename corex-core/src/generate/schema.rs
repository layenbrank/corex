use clap::{ArgAction, Parser};
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateSchedule {
    pub path: Vec<PathArgs>,

    #[serde(default)]
    pub uuid: Vec<UuidArgs>,

    #[serde(default)]
    pub file: Vec<FileArgs>,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    Path(PathArgs),
    Uuid(UuidArgs),
    File(FileArgs),
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct UuidArgs {
    #[arg(short, long, default_value = "1", help = "生成 UUID 的数量")]
    pub count: usize,

    #[arg(long, action = ArgAction::SetTrue, help = "以大写形式输出")]
    pub uppercase: bool,

    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct PathArgs {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,

    #[arg(short, long)]
    pub to: String,

    // #[arg(short, long)]
    // pub recursive: bool,
    #[arg(long, help = "转换规则")]
    pub transform: String,

    #[arg(long, help = "起始索引")]
    #[serde(default)]
    pub index: usize,

    #[arg(long, help = "路径分隔符")]
    pub separator: String,

    #[arg(long, action = ArgAction::SetTrue, help = "填充索引")]
    pub pad: bool,

    #[arg(long, action = ArgAction::Append, value_delimiter = ',', help = "包含模式（白名单），可多次使用或逗号分隔"
	)]
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub includes: Vec<String>,

    #[arg(long, action = ArgAction::Append, value_delimiter = ',', help = "排除模式（黑名单），可多次使用或逗号分隔"
	)]
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub excludes: Vec<String>,

    #[arg(long, action = ArgAction::Append, value_delimiter = ',', help = "将某个规则转换为大写，可多次使用或逗号分隔"
	)]
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub uppercase: Vec<String>,

    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct FileArgs {
    /// 输出文件路径
    #[arg(short, long)]
    pub to: String,

    /// 模板文件路径（推荐）
    #[arg(short, long)]
    pub template: Option<String>,

    /// 直接传入内容（简单模式）
    #[arg(short, long)]
    pub fragment: Option<String>,

    /// 动态变量，格式 key=value（可多次使用）
    #[arg(long, value_parser = parse_key_val, action = ArgAction::Append)]
    #[serde(default)]
    pub variable: Vec<(String, String)>,

    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}

// 辅助解析函数
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("无效的 key=value 格式: {}", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}
