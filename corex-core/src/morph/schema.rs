use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

/// morph（PDF）子命令
///
/// Args 变体名与 service `to*` 对齐（IPC action = kebab-case）：
/// `Render`→`render`，`Document`→`document`，`Remove`→`remove` …
///
/// 字段：snake_case；分页语义用 `count` / `limit` / `offset`。
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 读取 PDF 元数据
    Meta(MetaArgs),
    /// 渲染单页为 base64 PNG
    Render(RenderArgs),
    /// 渲染全部页缩略图
    Thumbnails(ThumbnailsArgs),
    /// 全文搜索
    Match(MatchArgs),
    /// 复制 PDF 到目标路径
    Export(ExportArgs),
    /// 合并多个 PDF
    Merge(MergeArgs),
    /// 拆分 PDF（模式见 `SplitMode`：`ranges` 或 `limit`）
    Split(SplitArgs),
    /// 导出为图片文件
    Images(ImagesArgs),
    /// 转换为 DOCX 或 XLSX
    Document(DocumentArgs),
    /// 按 0-based 页序重排并写出
    Reorder(ReorderArgs),
    /// 旋转指定页（0-based）并写出
    Rotate(RotateArgs),
    /// 删除指定页（0-based）并写出
    Remove(RemoveArgs),
    /// 抽取指定页（0-based）为新 PDF
    Extract(ExtractArgs),
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct MetaArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct RenderArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    /// 0-based 页偏移
    #[arg(long, default_value_t = 0)]
    pub offset: u32,
    #[arg(long, default_value_t = 2.0)]
    pub scale: f32,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ThumbnailsArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long, default_value_t = 0.5)]
    pub scale: f32,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct MatchArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long)]
    pub query: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ExportArgs {
    #[arg(long, value_parser = verifier::path)]
    pub src: String,
    #[arg(long)]
    pub dest: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct MergeArgs {
    #[arg(long, value_delimiter = ',')]
    pub paths: Vec<String>,
    #[arg(long)]
    pub dest: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct SplitArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long)]
    pub dir: String,
    /// 页码范围，格式 start-end（1-based，含首尾）；与 `limit` 二选一
    #[arg(long, value_delimiter = ',')]
    pub ranges: Option<Vec<String>>,
    /// 每个输出文件的页数上限；与 `ranges` 二选一
    #[arg(long)]
    pub limit: Option<u32>,
}

/// 拆分模式（由 `SplitArgs::mode` 从 ranges / limit 解析）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SplitMode {
    Ranges { ranges: Vec<String> },
    Limit { limit: u32 },
}

impl SplitArgs {
    /// 解析拆分模式：须且仅能指定 `ranges` 或 `limit` 之一
    pub fn mode(&self) -> anyhow::Result<SplitMode> {
        let has_ranges = self.ranges.as_ref().is_some_and(|r| !r.is_empty());
        match (has_ranges, self.limit) {
            (true, None) => Ok(SplitMode::Ranges {
                ranges: self.ranges.clone().unwrap_or_default(),
            }),
            (false, Some(limit)) => {
                if limit == 0 {
                    anyhow::bail!("limit 必须大于 0");
                }
                Ok(SplitMode::Limit { limit })
            }
            (true, Some(_)) => anyhow::bail!("ranges 与 limit 不能同时指定"),
            (false, None) => anyhow::bail!("须指定 ranges 或 limit 之一"),
        }
    }
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ImagesArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long, default_value_t = 2.0)]
    pub scale: f32,
    #[arg(long, default_value = "png")]
    pub format: String,
    #[arg(long)]
    pub dir: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct DocumentArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long)]
    pub format: String,
    #[arg(long)]
    pub dir: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ReorderArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    /// 重排后的 0-based 页序（须覆盖全部页且无重复）
    #[arg(long, value_delimiter = ',')]
    pub order: Vec<u32>,
    #[arg(long)]
    pub dest: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct RotateArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    /// 要旋转的页（0-based）
    #[arg(long, value_delimiter = ',')]
    pub pages: Vec<u32>,
    /// 旋转角度：90 / 180 / 270
    #[arg(long)]
    pub degrees: i32,
    #[arg(long)]
    pub dest: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct RemoveArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    /// 要删除的页（0-based）
    #[arg(long, value_delimiter = ',')]
    pub pages: Vec<u32>,
    #[arg(long)]
    pub dest: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ExtractArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    /// 要抽取的页（0-based，按给定顺序）
    #[arg(long, value_delimiter = ',')]
    pub pages: Vec<u32>,
    #[arg(long)]
    pub dest: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PdfMeta {
    pub path: String,
    pub title: String,
    pub author: String,
    /// 总页数
    pub count: u32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PageImage {
    pub base64: String,
    pub width: u32,
    pub height: u32,
    /// 0-based 页偏移
    pub offset: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
/// 全文搜索命中（`Match` 操作的返回项）
pub struct Hit {
    /// 命中页的 0-based 偏移
    pub offset: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub snippet: String,
}
