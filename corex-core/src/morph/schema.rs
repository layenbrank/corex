use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

/// morph（PDF）子命令
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 读取 PDF 元数据
    Meta(MetaArgs),
    /// 渲染单页为 base64 PNG
    RenderPage(RenderPageArgs),
    /// 渲染全部页缩略图
    RenderThumbnails(RenderThumbnailsArgs),
    /// 全文搜索
    Search(SearchArgs),
    /// 复制 PDF 到目标路径
    Export(ExportArgs),
    /// 合并多个 PDF
    Merge(MergeArgs),
    /// 按页码范围拆分 PDF
    Split(SplitArgs),
    /// 按固定页数拆分 PDF
    SplitByCount(SplitByCountArgs),
    /// 导出为图片文件
    ToImages(ToImagesArgs),
    /// 转换为 DOCX 或 XLSX
    ToOffice(ToOfficeArgs),
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct MetaArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct RenderPageArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long, default_value_t = 0)]
    pub page_index: u32,
    #[arg(long, default_value_t = 2.0)]
    pub scale: f32,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct RenderThumbnailsArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long, default_value_t = 0.5)]
    pub scale: f32,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct SearchArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long)]
    pub query: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ExportArgs {
    #[arg(long, value_parser = verifier::path)]
    pub src: String,
    #[arg(long, value_parser = verifier::path)]
    pub dest: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct MergeArgs {
    #[arg(long, value_delimiter = ',')]
    pub paths: Vec<String>,
    #[arg(long, value_parser = verifier::path)]
    pub dest: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct SplitArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    /// 页码范围，格式 start-end（1-based，含首尾）
    #[arg(long, value_delimiter = ',')]
    pub ranges: Vec<String>,
    #[arg(long)]
    pub dest_dir: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct SplitByCountArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long)]
    pub pages_per_file: u32,
    #[arg(long)]
    pub dest_dir: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ToImagesArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long, default_value_t = 2.0)]
    pub scale: f32,
    #[arg(long, default_value = "png")]
    pub format: String,
    #[arg(long)]
    pub dest_dir: String,
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub struct ToOfficeArgs {
    #[arg(long, value_parser = verifier::path)]
    pub path: String,
    #[arg(long)]
    pub format: String,
    #[arg(long)]
    pub dest_dir: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PdfMeta {
    pub path: String,
    pub title: String,
    pub author: String,
    pub page_count: u32,
    pub page_width: f32,
    pub page_height: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PageImage {
    pub data_base64: String,
    pub width: u32,
    pub height: u32,
    pub page_index: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchMatch {
    pub page_index: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub snippet: String,
}
