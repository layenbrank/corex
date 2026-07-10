use std::path::PathBuf;

use serde_json::Value;

/// Stage 间传递的数据单元
#[derive(Debug, Clone)]
pub enum PipelineItem {
    Path(PathBuf),
    Text(String),
    Json(Value),
    Empty,
}
