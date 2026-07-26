use clap::Parser;
use serde::{Deserialize, Serialize};

/// engine 子命令
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 获取 Bing 搜索建议
    Suggestion(SuggestionArgs),
}

fn default_one() -> String {
    "1".into()
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct SuggestionArgs {
    /// 页面类型（如 page.home）
    #[arg(long)]
    pub pt: String,

    /// 查询关键词
    #[arg(long)]
    pub qry: String,

    /// 光标位置（通常为 qry 长度）
    #[arg(long)]
    pub cp: u64,

    /// csr 标志
    #[arg(long, default_value = "1")]
    #[serde(default = "default_one")]
    pub csr: String,

    /// pths 标志
    #[arg(long, default_value = "1")]
    #[serde(default = "default_one")]
    pub pths: String,

    /// 客户端 CVID；省略时自动生成
    #[arg(long)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cvid: Option<String>,

    /// 出站 User-Agent；省略时使用默认 Chrome UA
    #[arg(long)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

/// Bing 建议条目（`t` 为任意类型码字符串，非固定 enum）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptySchema {
    pub id: String,
    pub q: String,
    pub u: String,
    pub t: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ISchema {
    pub ig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub s: Vec<EmptySchema>,
    pub i: ISchema,
}

/// 发往 Bing 的查询参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlParams {
    pub pt: String,
    pub qry: String,
    pub cp: u64,
    pub csr: String,
    pub pths: String,
    pub cvid: String,
}
