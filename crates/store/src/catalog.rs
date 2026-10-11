//! 指令库的机器可读目录：MCP 与 daemon 共用，禁止各抄一份。

use crate::{DirectiveMeta, DirectiveRecord, DirectiveStore, StoreError};
use serde_json::{Value, json};

/// 指令库摘要（默认档：无 YAML / definition）。
///
/// `names` 白名单过滤；`limit` 截断条数。
pub fn fetch_directives(
    store: &DirectiveStore,
    names: Option<&[String]>,
    limit: Option<usize>,
) -> Result<Value, StoreError> {
    let mut metas = store.metas()?;
    if let Some(allow) = names {
        metas.retain(|meta| allow.iter().any(|name| name == &meta.name));
    }
    if let Some(cap) = limit {
        metas.truncate(cap);
    }
    let directives: Vec<Value> = metas.iter().map(summary_of).collect();
    Ok(json!({
        "count": directives.len(),
        "directives": directives,
    }))
}

/// 单条完整文档：规范化 YAML + 模型（与 daemon `read_directive` 同形）。
pub fn fetch_directive(store: &DirectiveStore, name: &str) -> Result<Value, StoreError> {
    let record = store.fetch(name)?;
    Ok(document_of(record))
}

/// 摘要条目形状（daemon 可在其上再挂 `last_run`）。
pub fn summary_of(meta: &DirectiveMeta) -> Value {
    json!({
        "name": meta.name,
        "folder": meta.folder,
        "source": meta.source,
        "visible": meta.visible,
        "updated_at_ms": meta.updated_at_ms,
        "bucket": meta.summary.as_ref().and_then(|summary| summary.bucket.clone()),
        "summary": meta.summary.as_ref().map(|summary| json!({
            "description": summary.description,
            "step_count": summary.step_count,
            "input_count": summary.input_count,
            "trigger_count": summary.trigger_count,
            "has_cron": summary.has_cron,
            "has_watch": summary.has_watch,
        })),
    })
}

/// 完整指令文档。
pub fn document_of(record: DirectiveRecord) -> Value {
    json!({
        "name": record.name,
        "folder": record.folder,
        "source": record.source,
        "visible": record.visible,
        "created_at_ms": record.created_at_ms,
        "updated_at_ms": record.updated_at_ms,
        "yaml": record.yaml,
        "definition": record.definition,
    })
}
