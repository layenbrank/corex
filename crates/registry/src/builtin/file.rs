//! File actions: write / read / update / remove (+ copy).

use crate::ActionRegistry;
use crate::builtin::util::{confine_path, opt_bool, opt_str, require_map, require_str};
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use regex::Regex;
use ropey::Rope;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

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
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
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

fn apply_newline(
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

/// 1-based inclusive line window → 0-based [start, end) line indices in Rope.
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
    // Rope counts a trailing empty line after final `\n`; treat empty file as 0.
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
    m.insert("lines".into(), Value::List(lines));
    Value::Map(m)
}

fn splice(
    content: &str,
    start: &str,
    end: &str,
    replacement: &str,
    nth: usize,
    include_markers: bool,
    on_missing: &str,
) -> Result<(String, bool), ActionError> {
    if nth == 0 {
        return Err(ActionError::InvalidParams(
            "nth 为 1-based，不能为 0".into(),
        ));
    }
    let mut from = 0usize;
    let mut start_pos = None;
    for i in 1..=nth {
        match content[from..].find(start) {
            Some(rel) => {
                let abs = from + rel;
                if i == nth {
                    start_pos = Some(abs);
                    break;
                }
                from = abs + start.len();
            }
            None => {
                start_pos = None;
                break;
            }
        }
    }
    let Some(start_pos) = start_pos else {
        return match on_missing {
            "noop" => Ok((content.to_string(), false)),
            "error" => Err(ActionError::execution(format!(
                "未找到第 {nth} 个起始 marker: {start}"
            ))),
            other => Err(ActionError::InvalidParams(format!(
                "不支持的 on_missing: {other}（error|noop）"
            ))),
        };
    };
    let after_start = start_pos + start.len();
    let end_rel = content[after_start..].find(end);
    let Some(end_rel) = end_rel else {
        return match on_missing {
            "noop" => Ok((content.to_string(), false)),
            "error" => Err(ActionError::execution(format!("未找到结束 marker: {end}"))),
            other => Err(ActionError::InvalidParams(format!(
                "不支持的 on_missing: {other}（error|noop）"
            ))),
        };
    };
    let end_abs = after_start + end_rel;
    let (cut_from, cut_to) = if include_markers {
        (start_pos, end_abs + end.len())
    } else {
        (after_start, end_abs)
    };
    let mut out = String::with_capacity(content.len() + replacement.len());
    out.push_str(&content[..cut_from]);
    out.push_str(replacement);
    out.push_str(&content[cut_to..]);
    let changed = out != content;
    Ok((out, changed))
}

fn str_replace_exact(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Result<(String, usize), ActionError> {
    if old.is_empty() {
        return Err(ActionError::InvalidParams("old 不能为空".into()));
    }
    let matches = content.matches(old).count();
    if matches == 0 {
        return Err(ActionError::execution(format!("未找到要替换的文本: {old}")));
    }
    if !replace_all && matches > 1 {
        return Err(ActionError::execution(format!(
            "old 匹配 {matches} 处，默认要求唯一；可设 replace_all: true"
        )));
    }
    let out = if replace_all {
        content.replace(old, new)
    } else {
        content.replacen(old, new, 1)
    };
    Ok((out, matches))
}

fn apply_regex(
    content: &str,
    pattern: &str,
    replacement: &str,
) -> Result<(String, usize), ActionError> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        return Err(ActionError::InvalidParams(format!(
            "regex pattern 超过 {MAX_REGEX_PATTERN_LEN} 字符"
        )));
    }
    let re =
        Regex::new(pattern).map_err(|e| ActionError::InvalidParams(format!("无效 regex: {e}")))?;
    let matches = re.find_iter(content).count();
    let out = re.replace_all(content, replacement).into_owned();
    if out.len() > MAX_REGEX_REPLACE_BYTES {
        return Err(ActionError::execution(format!(
            "regex 替换结果超过 {MAX_REGEX_REPLACE_BYTES} 字节"
        )));
    }
    Ok((out, matches))
}

fn apply_json_set(existing: &str, pointer: &str, value: &Value) -> Result<String, ActionError> {
    let mut root: Value = if existing.trim().is_empty() {
        Value::Map(BTreeMap::new())
    } else {
        let json: serde_json::Value = serde_json::from_str(existing)
            .map_err(|e| ActionError::execution(format!("JSON 解析失败: {e}")))?;
        Value::from_json(json)
    };
    set_dot_path(&mut root, pointer, value.clone())?;
    let json = root.to_json();
    serde_json::to_string_pretty(&json)
        .map_err(|e| ActionError::execution(format!("JSON 序列化失败: {e}")))
}

fn set_dot_path(val: &mut Value, path: &str, new_value: Value) -> Result<(), ActionError> {
    if path.is_empty() {
        *val = new_value;
        return Ok(());
    }
    let mut parts: Vec<&str> = path.split('.').collect();
    let last = parts
        .pop()
        .ok_or_else(|| ActionError::InvalidParams("空 pointer".into()))?;
    let mut current = val;
    for segment in parts {
        match current {
            Value::Map(m) => {
                if !m.contains_key(segment) {
                    m.insert(segment.to_string(), Value::Map(BTreeMap::new()));
                }
                current = m
                    .get_mut(segment)
                    .ok_or_else(|| ActionError::execution(format!("无法设置路径: {path}")))?;
            }
            _ => {
                return Err(ActionError::execution(format!(
                    "路径 {path} 中间节点不是对象"
                )));
            }
        }
    }
    match current {
        Value::Map(m) => {
            m.insert(last.to_string(), new_value);
            Ok(())
        }
        Value::List(l) => {
            let idx: usize = last
                .parse()
                .map_err(|_| ActionError::InvalidParams(format!("无效列表索引: {last}")))?;
            if idx >= l.len() {
                return Err(ActionError::execution(format!("列表索引越界: {idx}")));
            }
            l[idx] = new_value;
            Ok(())
        }
        _ => Err(ActionError::execution(format!("无法在路径 {path} 设置值"))),
    }
}

fn apply_unified_patch(base: &str, diff: &str) -> Result<String, ActionError> {
    let patch = diffy::Patch::from_str(diff)
        .map_err(|e| ActionError::InvalidParams(format!("无效 patch: {e}")))?;
    diffy::apply(base, &patch).map_err(|e| ActionError::execution(format!("应用 patch 失败: {e}")))
}

fn line_char_range(
    rope: &Rope,
    start_line: usize,
    end_line: usize,
) -> Result<(usize, usize), ActionError> {
    let total = rope.len_lines();
    if start_line == 0 || end_line == 0 {
        return Err(ActionError::InvalidParams(
            "行号为 1-based，不能为 0".into(),
        ));
    }
    if end_line < start_line {
        return Err(ActionError::InvalidParams(
            "end_line 不能小于 start_line".into(),
        ));
    }
    let start0 = start_line - 1;
    if start0 >= total {
        return Err(ActionError::InvalidParams(format!(
            "start_line {start_line} 超出总行数 {total}"
        )));
    }
    let end0_excl = end_line.min(total);
    let a = rope.line_to_char(start0);
    let b = if end0_excl >= total {
        rope.len_chars()
    } else {
        rope.line_to_char(end0_excl)
    };
    Ok((a, b))
}

fn rope_replace_lines(
    rope: &mut Rope,
    start_line: usize,
    end_line: usize,
    content: &str,
) -> Result<(usize, usize, usize), ActionError> {
    let (a, b) = line_char_range(rope, start_line, end_line)?;
    rope.remove(a..b);
    rope.insert(a, content);
    let affected = end_line - start_line + 1;
    Ok((start_line, end_line, affected))
}

fn rope_delete_lines(
    rope: &mut Rope,
    start_line: usize,
    end_line: usize,
) -> Result<(usize, usize, usize), ActionError> {
    let (a, b) = line_char_range(rope, start_line, end_line)?;
    rope.remove(a..b);
    let affected = end_line - start_line + 1;
    Ok((start_line, end_line, affected))
}

fn rope_insert_lines(
    rope: &mut Rope,
    after_line: usize,
    content: &str,
) -> Result<(usize, usize, usize), ActionError> {
    let total = rope.len_lines();
    let char_idx = if after_line == 0 {
        0
    } else if after_line >= total {
        rope.len_chars()
    } else {
        // after_line is 1-based: insert at start of the next line
        rope.line_to_char(after_line)
    };
    rope.insert(char_idx, content);
    let inserted = content.lines().count().max(1);
    let start = after_line + 1;
    Ok((start, after_line + inserted, inserted))
}

struct WriteMeta {
    matches: Option<i64>,
    start_line: Option<i64>,
    end_line: Option<i64>,
    lines_affected: Option<i64>,
    newline: Option<String>,
}

impl Default for WriteMeta {
    fn default() -> Self {
        Self {
            matches: None,
            start_line: None,
            end_line: None,
            lines_affected: None,
            newline: None,
        }
    }
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

#[async_trait]
impl Action for FileRead {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.read",
            "File Read",
            "读取文件内容、行窗，或轻量 exists/stat",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("mode", SchemaType::Str, false)
                .with_default("content")
                .with_description("content | lines | stat | exists"),
            ParamSchema::new("start_line", SchemaType::Int, false),
            ParamSchema::new("end_line", SchemaType::Int, false),
            ParamSchema::new("limit", SchemaType::Int, false),
            ParamSchema::new("max_bytes", SchemaType::Int, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        let mode = opt_str(map, "mode").unwrap_or_else(|| "content".into());
        let max_bytes = opt_usize(map, "max_bytes")?.unwrap_or(DEFAULT_MAX_READ_BYTES);

        match mode.as_str() {
            "content" | "lines" => {
                let text = tokio::fs::read_to_string(&path).await.map_err(|e| {
                    ActionError::execution(format!("读取文件失败 {}: {e}", path.display()))
                })?;
                enforce_max_bytes(&text, max_bytes)?;
                let rope = Rope::from_str(&text);
                let start_line = opt_usize(map, "start_line")?;
                let end_line = opt_usize(map, "end_line")?;
                let limit = opt_usize(map, "limit")?;
                let (start0, end0) =
                    if start_line.is_some() || end_line.is_some() || limit.is_some() {
                        line_window(&rope, start_line, end_line, limit)?
                    } else if mode == "lines" {
                        line_window(&rope, Some(1), None, None)?
                    } else {
                        (0, 0)
                    };

                if mode == "lines" {
                    Ok(lines_value(&rope, start0, end0))
                } else if start_line.is_some() || end_line.is_some() || limit.is_some() {
                    Ok(Value::Str(slice_lines_text(&rope, start0, end0)))
                } else {
                    Ok(Value::Str(text))
                }
            }
            "exists" => {
                let exists = tokio::fs::try_exists(&path).await.map_err(|e| {
                    ActionError::execution(format!("检查存在失败 {}: {e}", path.display()))
                })?;
                Ok(Value::Bool(exists))
            }
            "stat" => {
                let meta = tokio::fs::metadata(&path).await.map_err(|e| {
                    ActionError::execution(format!("读取元数据失败 {}: {e}", path.display()))
                })?;
                Ok(stat_value(path, meta))
            }
            other => Err(ActionError::InvalidParams(format!(
                "不支持的 file.read mode: {other}"
            ))),
        }
    }
}

#[async_trait]
impl Action for FileWrite {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.write",
            "File Write",
            "写入或局部更新文本/JSON 文件（迷你 IDE）",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("content", SchemaType::Str, false),
            ParamSchema::new("mode", SchemaType::Str, false)
                .with_default("overwrite")
                .with_description(
                    "overwrite | append | str_replace | replace_lines | insert_lines | delete_lines | splice | regex | json_set | patch",
                ),
            ParamSchema::new("start", SchemaType::Str, false),
            ParamSchema::new("end", SchemaType::Str, false),
            ParamSchema::new("old", SchemaType::Str, false),
            ParamSchema::new("new", SchemaType::Str, false),
            ParamSchema::new("replace_all", SchemaType::Bool, false).with_default(false),
            ParamSchema::new("start_line", SchemaType::Int, false),
            ParamSchema::new("end_line", SchemaType::Int, false),
            ParamSchema::new("after_line", SchemaType::Int, false),
            ParamSchema::new("nth", SchemaType::Int, false).with_default(1),
            ParamSchema::new("include_markers", SchemaType::Bool, false).with_default(false),
            ParamSchema::new("on_missing", SchemaType::Str, false)
                .with_default("error")
                .with_description("error | noop"),
            ParamSchema::new("pattern", SchemaType::Str, false),
            ParamSchema::new("replacement", SchemaType::Str, false),
            ParamSchema::new("pointer", SchemaType::Str, false),
            ParamSchema::new("value", SchemaType::Any, false),
            ParamSchema::new("diff", SchemaType::Str, false),
            ParamSchema::new("newline", SchemaType::Str, false)
                .with_default("preserve")
                .with_description("preserve | lf | crlf"),
            ParamSchema::new("create_dirs", SchemaType::Bool, false).with_default(true),
            ParamSchema::new("backup", SchemaType::Bool, false).with_default(false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        let mode = opt_str(map, "mode").unwrap_or_else(|| "overwrite".into());
        let create_dirs = opt_bool(map, "create_dirs", true);
        let backup = opt_bool(map, "backup", false);
        let newline_mode = opt_str(map, "newline").unwrap_or_else(|| "preserve".into());

        if create_dirs {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await?;
                }
            }
        }

        let existing = if mode == "overwrite" {
            String::new()
        } else {
            tokio::fs::read_to_string(&path).await.unwrap_or_default()
        };

        let mut meta = WriteMeta::default();
        let (raw_content, changed) = match mode.as_str() {
            "overwrite" => {
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                (content.to_string(), true)
            }
            "append" => {
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let mut rope = Rope::from_str(&existing);
                rope.insert(rope.len_chars(), content);
                (rope.to_string(), true)
            }
            "str_replace" => {
                let old = require_str(map, "old")?;
                let new = map
                    .get("new")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("new".into()))?;
                let replace_all = opt_bool(map, "replace_all", false);
                let (out, matches) = str_replace_exact(&existing, &old, new, replace_all)?;
                meta.matches = Some(matches as i64);
                let changed = out != existing;
                (out, changed)
            }
            "replace_lines" => {
                let start_line = opt_usize(map, "start_line")?
                    .ok_or_else(|| ActionError::MissingParam("start_line".into()))?;
                let end_line = opt_usize(map, "end_line")?.unwrap_or(start_line);
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let mut rope = Rope::from_str(&existing);
                let (s, e, n) = rope_replace_lines(&mut rope, start_line, end_line, content)?;
                meta.start_line = Some(s as i64);
                meta.end_line = Some(e as i64);
                meta.lines_affected = Some(n as i64);
                let out = rope.to_string();
                let changed = out != existing;
                (out, changed)
            }
            "insert_lines" => {
                let after_line = opt_usize(map, "after_line")?
                    .ok_or_else(|| ActionError::MissingParam("after_line".into()))?;
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let mut rope = Rope::from_str(&existing);
                let (s, e, n) = rope_insert_lines(&mut rope, after_line, content)?;
                meta.start_line = Some(s as i64);
                meta.end_line = Some(e as i64);
                meta.lines_affected = Some(n as i64);
                (rope.to_string(), true)
            }
            "delete_lines" => {
                let start_line = opt_usize(map, "start_line")?
                    .ok_or_else(|| ActionError::MissingParam("start_line".into()))?;
                let end_line = opt_usize(map, "end_line")?.unwrap_or(start_line);
                let mut rope = Rope::from_str(&existing);
                let (s, e, n) = rope_delete_lines(&mut rope, start_line, end_line)?;
                meta.start_line = Some(s as i64);
                meta.end_line = Some(e as i64);
                meta.lines_affected = Some(n as i64);
                let out = rope.to_string();
                let changed = out != existing;
                (out, changed)
            }
            "splice" => {
                let start = require_str(map, "start")?;
                let end = require_str(map, "end")?;
                let replacement = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let nth = opt_usize(map, "nth")?.unwrap_or(1);
                let include_markers = opt_bool(map, "include_markers", false);
                let on_missing = opt_str(map, "on_missing").unwrap_or_else(|| "error".into());
                splice(
                    &existing,
                    &start,
                    &end,
                    replacement,
                    nth,
                    include_markers,
                    &on_missing,
                )?
            }
            "regex" => {
                let pattern = require_str(map, "pattern")?;
                let replacement = opt_str(map, "replacement").unwrap_or_default();
                let (out, matches) = apply_regex(&existing, &pattern, &replacement)?;
                meta.matches = Some(matches as i64);
                let changed = out != existing;
                (out, changed)
            }
            "json_set" => {
                let pointer = require_str(map, "pointer")?;
                let value = map
                    .get("value")
                    .cloned()
                    .or_else(|| map.get("content").cloned())
                    .ok_or_else(|| ActionError::MissingParam("value".into()))?;
                let out = apply_json_set(&existing, &pointer, &value)?;
                let changed = out != existing;
                (out, changed)
            }
            "patch" => {
                let diff = require_str(map, "diff")?;
                let out = apply_unified_patch(&existing, &diff)?;
                let changed = out != existing;
                (out, changed)
            }
            "replace_between" => {
                return Err(ActionError::InvalidParams(
                    "file.write mode `replace_between` 已重命名为 `splice`".into(),
                ));
            }
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的 file.write mode: {other}"
                )));
            }
        };

        let (final_content, nl) = apply_newline(raw_content, &newline_mode, &existing)?;
        meta.newline = Some(nl);
        let bytes = final_content.as_bytes();
        if changed || mode == "overwrite" {
            atomic_write(&path, bytes, backup).await?;
        }
        Ok(write_result(path, changed, bytes.len(), meta))
    }
}

#[async_trait]
impl Action for FileCopy {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.copy",
            "File Copy",
            "复制文件（单文件 std::fs::copy）",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let from = require_path(&params, "from")?;
        let to = require_path(&params, "to")?;
        let from = confine_path(ctx, &from)?;
        let to = confine_path(ctx, &to)?;
        if let Some(parent) = to.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        tokio::fs::copy(&from, &to)
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        Ok(Value::File(to))
    }
}

#[async_trait]
impl Action for FileUpdate {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.update",
            "File Update",
            "重命名或移动文件",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("from", SchemaType::File, true),
            ParamSchema::new("to", SchemaType::File, true),
            ParamSchema::new("create_dirs", SchemaType::Bool, false).with_default(true),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let from = require_path(&params, "from")?;
        let to = require_path(&params, "to")?;
        let from = confine_path(ctx, &from)?;
        let to = confine_path(ctx, &to)?;
        let create_dirs = opt_bool(map, "create_dirs", true);
        if create_dirs {
            if let Some(parent) = to.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await?;
                }
            }
        }
        tokio::fs::rename(&from, &to)
            .await
            .map_err(|e| ActionError::execution(format!("移动失败: {e}")))?;
        Ok(Value::File(to))
    }
}

#[async_trait]
impl Action for FileRemove {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.remove",
            "File Remove",
            "删除文件（目录则递归删除）",
            ActionCategory::Data,
        )
        .with_params(vec![ParamSchema::new("path", SchemaType::File, true)])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await?;
        } else {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(Value::Bool(true))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(FileRead));
    registry.register(Arc::new(FileWrite));
    registry.register(Arc::new(FileCopy));
    registry.register(Arc::new(FileUpdate));
    registry.register(Arc::new(FileRemove));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;
    use tempfile::tempdir;

    #[tokio::test]
    async fn overwrite_and_splice() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let mut ctx = ExecutionContext::default();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert(
            "content".into(),
            Value::Str("<!--START-->old<!--END-->".into()),
        );
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("splice".into()));
        params.insert("start".into(), Value::Str("<!--START-->".into()));
        params.insert("end".into(), Value::Str("<!--END-->".into()));
        params.insert("content".into(), Value::Str("new".into()));
        let out = FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(
            out.as_map()
                .unwrap()
                .get("changed")
                .unwrap()
                .as_bool()
                .unwrap()
        );

        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(text, "<!--START-->new<!--END-->");
    }

    #[tokio::test]
    async fn splice_nth_and_on_missing_noop() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.txt");
        tokio::fs::write(&path, "<<a>>x<<b>>").await.unwrap();
        let mut ctx = ExecutionContext::default();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("splice".into()));
        params.insert("start".into(), Value::Str("<<".into()));
        params.insert("end".into(), Value::Str(">>".into()));
        params.insert("content".into(), Value::Str("Y".into()));
        params.insert("nth".into(), Value::Int(2));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(text, "<<a>>x<<Y>>");

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("splice".into()));
        params.insert("start".into(), Value::Str("NOPE".into()));
        params.insert("end".into(), Value::Str("X".into()));
        params.insert("content".into(), Value::Str("Z".into()));
        params.insert("on_missing".into(), Value::Str("noop".into()));
        let out = FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(
            !out.as_map()
                .unwrap()
                .get("changed")
                .unwrap()
                .as_bool()
                .unwrap()
        );
    }

    #[tokio::test]
    async fn append_str_replace_and_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("edit.txt");
        let mut ctx = ExecutionContext::default();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("content".into(), Value::Str("a\nb\nc\n".into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("append".into()));
        params.insert("content".into(), Value::Str("d\n".into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("str_replace".into()));
        params.insert("old".into(), Value::Str("b\n".into()));
        params.insert("new".into(), Value::Str("B\n".into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("replace_lines".into()));
        params.insert("start_line".into(), Value::Int(1));
        params.insert("end_line".into(), Value::Int(1));
        params.insert("content".into(), Value::Str("A\n".into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("insert_lines".into()));
        params.insert("after_line".into(), Value::Int(0));
        params.insert("content".into(), Value::Str("Z\n".into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("delete_lines".into()));
        params.insert("start_line".into(), Value::Int(4));
        params.insert("end_line".into(), Value::Int(4));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(text, "Z\nA\nB\nd\n");
    }

    #[tokio::test]
    async fn str_replace_requires_unique() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("u.txt");
        tokio::fs::write(&path, "xx").await.unwrap();
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("str_replace".into()));
        params.insert("old".into(), Value::Str("x".into()));
        params.insert("new".into(), Value::Str("y".into()));
        let err = FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("匹配"), "got: {err}");
    }

    #[tokio::test]
    async fn newline_crlf() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nl.txt");
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("content".into(), Value::Str("a\nb\n".into()));
        params.insert("newline".into(), Value::Str("crlf".into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let bytes = tokio::fs::read(&path).await.unwrap();
        assert_eq!(bytes, b"a\r\nb\r\n");
    }

    #[tokio::test]
    async fn read_lines_with_limit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("r.txt");
        tokio::fs::write(&path, "a\nb\nc\nd\n").await.unwrap();
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("lines".into()));
        params.insert("start_line".into(), Value::Int(2));
        params.insert("limit".into(), Value::Int(2));
        let out = FileRead
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let m = out.as_map().unwrap();
        let lines = m.get("lines").unwrap().as_list().unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0]
                .as_map()
                .unwrap()
                .get("text")
                .unwrap()
                .as_str()
                .unwrap(),
            "b"
        );
        assert_eq!(
            lines[1]
                .as_map()
                .unwrap()
                .get("text")
                .unwrap()
                .as_str()
                .unwrap(),
            "c"
        );
    }

    #[tokio::test]
    async fn patch_unified() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("p.txt");
        tokio::fs::write(&path, "hello\n").await.unwrap();
        let mut ctx = ExecutionContext::default();
        let diff = "\
--- a/p.txt
+++ b/p.txt
@@ -1 +1 @@
-hello
+world
";
        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("patch".into()));
        params.insert("diff".into(), Value::Str(diff.into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(text, "world\n");
    }

    #[tokio::test]
    async fn json_set_mode() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cfg.json");
        tokio::fs::write(&path, r#"{"build":{"version":"0.1.0"}}"#)
            .await
            .unwrap();
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("json_set".into()));
        params.insert("pointer".into(), Value::Str("build.version".into()));
        params.insert("value".into(), Value::Str("1.0.0".into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(text.contains("1.0.0"));
    }

    #[tokio::test]
    async fn filesystem_roots_rejects_outside() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("allowed");
        std::fs::create_dir_all(&root).unwrap();
        let outside = dir.path().join("denied");
        std::fs::create_dir_all(&outside).unwrap();
        let file = outside.join("x.txt");
        std::fs::write(&file, b"x").unwrap();

        let mut cfg = corex_core::RuntimeConfig::default();
        cfg.filesystem_roots = vec![root];
        let mut ctx = ExecutionContext::new(cfg);

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(file.display().to_string()));
        params.insert("content".into(), Value::Str("nope".into()));
        let err = FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("越界") || msg.contains("不在") || msg.contains("无法解析"),
            "got: {msg}"
        );
    }

    #[tokio::test]
    async fn regex_mode_rejects_long_pattern() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.txt");
        tokio::fs::write(&path, "hello").await.unwrap();
        let mut ctx = ExecutionContext::default();
        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("regex".into()));
        params.insert("pattern".into(), Value::Str("a".repeat(2000)));
        params.insert("replacement".into(), Value::Str("b".into()));
        let err = FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("1024") || err.to_string().contains("pattern"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn read_exists_and_stat() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("x.txt");
        tokio::fs::write(&path, b"hi").await.unwrap();
        let mut ctx = ExecutionContext::default();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("exists".into()));
        let exists = FileRead
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(exists.as_bool().unwrap());

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("stat".into()));
        let stat = FileRead
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        let m = stat.as_map().unwrap();
        assert_eq!(m.get("kind").unwrap().as_str().unwrap(), "file");
        assert_eq!(m.get("size").unwrap().as_i64().unwrap(), 2);
    }

    #[tokio::test]
    async fn update_rename_and_remove() {
        let dir = tempdir().unwrap();
        let from = dir.path().join("a.txt");
        let to = dir.path().join("b.txt");
        tokio::fs::write(&from, b"x").await.unwrap();
        let mut ctx = ExecutionContext::default();

        let mut params = BTreeMap::new();
        params.insert("from".into(), Value::Str(from.display().to_string()));
        params.insert("to".into(), Value::Str(to.display().to_string()));
        FileUpdate
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(!from.exists());
        assert!(to.exists());

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(to.display().to_string()));
        FileRemove
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(!to.exists());
    }
}
