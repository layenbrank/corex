//! 文件动作：写入 / 读取 / 更新 / 删除（外加复制）。

use crate::ActionRegistry;
pub(crate) use crate::builtin::util::{confine_path, opt_bool, opt_str, require_map, require_str};
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Value,
};
use regex::Regex;
use ropey::Rope;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

mod mode;
mod ops;
mod read;
mod write;

const MAX_REGEX_PATTERN_LEN: usize = 1024;
const MAX_REGEX_REPLACE_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_READ_BYTES: usize = 32 * 1024 * 1024;

fn require_path(params: &Value, key: &str) -> Result<PathBuf, ActionError> {
    params
        .as_map()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| ActionError::MissingParam(key.into()))
}

fn opt_usize(map: &BTreeMap<String, Value>, key: &str) -> Result<Option<usize>, ActionError> {
    match map.get(key) {
        None => Ok(None),
        Some(v) => {
            let n = v
                .as_i64()
                .ok_or_else(|| ActionError::InvalidParams(format!("{key} 须为整数")))?;
            if n < 0 {
                return Err(ActionError::InvalidParams(format!("{key} 不能为负")));
            }
            Ok(Some(n as usize))
        }
    }
}

async fn atomic_write(path: &Path, content: &[u8], backup: bool) -> Result<(), ActionError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    if backup && path.is_file() {
        let bak = path.with_extension(format!(
            "{}.bak",
            path.extension().and_then(|e| e.to_str()).unwrap_or("")
        ));
        tokio::fs::copy(path, &bak)
            .await
            .map_err(|e| ActionError::execution(format!("创建备份失败 {}: {e}", bak.display())))?;
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let tmp = parent.join(format!(".corex-write-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&tmp, content).await?;
    tokio::fs::rename(&tmp, path).await.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ActionError::execution(format!("原子写入失败 {}: {e}", path.display()))
    })?;
    Ok(())
}

fn detect_newline(s: &str) -> &'static str {
    if s.contains("\r\n") { "crlf" } else { "lf" }
}

fn with_newline(
    content: String,
    mode: &str,
    original: &str,
) -> Result<(String, String), ActionError> {
    let style = match mode {
        "preserve" => detect_newline(if original.is_empty() {
            &content
        } else {
            original
        }),
        "lf" | "crlf" => mode,
        other => {
            return Err(ActionError::InvalidParams(format!(
                "不支持的 newline: {other}（preserve|lf|crlf）"
            )));
        }
    };
    let normalized = match style {
        "lf" => content.replace("\r\n", "\n").replace('\r', "\n"),
        "crlf" => {
            let lf = content.replace("\r\n", "\n").replace('\r', "\n");
            lf.replace('\n', "\r\n")
        }
        _ => content,
    };
    Ok((normalized, style.to_string()))
}

fn enforce_max_bytes(text: &str, max_bytes: usize) -> Result<(), ActionError> {
    if text.len() > max_bytes {
        return Err(ActionError::execution(format!(
            "文件超过 max_bytes ({max_bytes})，实际 {} 字节",
            text.len()
        )));
    }
    Ok(())
}

/// 1-based 闭区间行窗口 → Rope 里 0-based 的 [start, end) 行下标。
fn line_window(
    rope: &Rope,
    start_line: Option<usize>,
    end_line: Option<usize>,
    limit: Option<usize>,
) -> Result<(usize, usize), ActionError> {
    let total = rope.len_lines();
    if total == 0 {
        return Ok((0, 0));
    }
    // 末尾 `\n` 之后 Rope 会算一个空行；空文件按 0 行处理。
    let start = start_line.unwrap_or(1);
    if start == 0 {
        return Err(ActionError::InvalidParams(
            "start_line 为 1-based，不能为 0".into(),
        ));
    }
    let start0 = start - 1;
    if start0 >= total {
        return Err(ActionError::InvalidParams(format!(
            "start_line {start} 超出总行数 {total}"
        )));
    }
    let end0_excl = if let Some(lim) = limit {
        start0.saturating_add(lim).min(total)
    } else if let Some(end) = end_line {
        if end == 0 {
            return Err(ActionError::InvalidParams(
                "end_line 为 1-based，不能为 0".into(),
            ));
        }
        if end < start {
            return Err(ActionError::InvalidParams(
                "end_line 不能小于 start_line".into(),
            ));
        }
        end.min(total)
    } else {
        total
    };
    Ok((start0, end0_excl))
}

fn slice_lines_text(rope: &Rope, start0: usize, end0_excl: usize) -> String {
    if start0 >= end0_excl {
        return String::new();
    }
    let a = rope.line_to_char(start0);
    let b = if end0_excl >= rope.len_lines() {
        rope.len_chars()
    } else {
        rope.line_to_char(end0_excl)
    };
    rope.slice(a..b).to_string()
}

fn lines_value(rope: &Rope, start0: usize, end0_excl: usize) -> Value {
    let mut lines = Vec::new();
    for i in start0..end0_excl {
        let mut text = rope.line(i).to_string();
        if text.ends_with("\r\n") {
            text.truncate(text.len() - 2);
        } else if text.ends_with('\n') || text.ends_with('\r') {
            text.pop();
        }
        let mut row = BTreeMap::new();
        row.insert("line".into(), Value::Int((i + 1) as i64));
        row.insert("text".into(), Value::Str(text));
        lines.push(Value::Map(row));
    }
    let mut m = BTreeMap::new();
    m.insert("total_lines".into(), Value::Int(rope.len_lines() as i64));
    m.insert(
        "start_line".into(),
        Value::Int(if end0_excl > start0 {
            (start0 + 1) as i64
        } else {
            0
        }),
    );
    m.insert(
        "end_line".into(),
        Value::Int(if end0_excl > start0 {
            end0_excl as i64
        } else {
            0
        }),
    );
    m.insert("lines".into(), Value::Array(lines));
    Value::Map(m)
}

#[derive(Default)]
struct WriteMeta {
    matches: Option<i64>,
    start_line: Option<i64>,
    end_line: Option<i64>,
    lines_affected: Option<i64>,
    newline: Option<String>,
}

fn write_result(path: PathBuf, changed: bool, bytes_written: usize, meta: WriteMeta) -> Value {
    let mut m = BTreeMap::new();
    m.insert("path".into(), Value::File(path));
    m.insert("changed".into(), Value::Bool(changed));
    m.insert("bytes_written".into(), Value::Int(bytes_written as i64));
    if let Some(v) = meta.matches {
        m.insert("matches".into(), Value::Int(v));
    }
    if let Some(v) = meta.start_line {
        m.insert("start_line".into(), Value::Int(v));
    }
    if let Some(v) = meta.end_line {
        m.insert("end_line".into(), Value::Int(v));
    }
    if let Some(v) = meta.lines_affected {
        m.insert("lines_affected".into(), Value::Int(v));
    }
    if let Some(v) = meta.newline {
        m.insert("newline".into(), Value::Str(v));
    }
    Value::Map(m)
}

fn entry_kind(meta: &std::fs::Metadata) -> &'static str {
    if meta.is_dir() {
        "dir"
    } else if meta.is_file() {
        "file"
    } else {
        "other"
    }
}

fn modified_unix(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified().ok().and_then(|t| {
        t.duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    })
}

fn stat_value(path: PathBuf, meta: std::fs::Metadata) -> Value {
    let mut m = BTreeMap::new();
    m.insert("path".into(), Value::File(path));
    m.insert("kind".into(), Value::Str(entry_kind(&meta).into()));
    m.insert("size".into(), Value::Int(meta.len() as i64));
    m.insert(
        "readonly".into(),
        Value::Bool(meta.permissions().readonly()),
    );
    if let Some(ts) = modified_unix(&meta) {
        m.insert("modified".into(), Value::Int(ts));
    }
    Value::Map(m)
}

pub struct FileRead;
pub struct FileWrite;
pub struct FileCopy;
pub struct FileUpdate;
pub struct FileRemove;

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(FileRead));
    registry.register(Arc::new(FileWrite));
    registry.register(Arc::new(FileCopy));
    registry.register(Arc::new(FileUpdate));
    registry.register(Arc::new(FileRemove));
}

#[cfg(test)]
mod tests;
