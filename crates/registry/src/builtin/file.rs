//! File actions: read / write / copy / delete.

use crate::builtin::util::{confine_path, opt_bool, opt_str, require_map, require_str};
use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_REGEX_PATTERN_LEN: usize = 1024;
const MAX_REGEX_REPLACE_BYTES: usize = 8 * 1024 * 1024;

fn require_path(params: &Value, key: &str) -> Result<PathBuf, ActionError> {
    params
        .as_map()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| ActionError::MissingParam(key.into()))
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
            path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
        ));
        tokio::fs::copy(path, &bak).await.map_err(|e| {
            ActionError::execution(format!("创建备份失败 {}: {e}", bak.display()))
        })?;
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let tmp = parent.join(format!(
        ".corex-write-{}",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::write(&tmp, content).await?;
    tokio::fs::rename(&tmp, path).await.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ActionError::execution(format!("原子写入失败 {}: {e}", path.display()))
    })?;
    Ok(())
}

fn replace_between(content: &str, start: &str, end: &str, replacement: &str) -> Result<String, ActionError> {
    let start_pos = content
        .find(start)
        .ok_or_else(|| ActionError::execution(format!("未找到起始 marker: {start}")))?;
    let after_start = start_pos + start.len();
    let end_pos = content[after_start..]
        .find(end)
        .ok_or_else(|| ActionError::execution(format!("未找到结束 marker: {end}")))?;
    let end_abs = after_start + end_pos;
    let mut out = String::with_capacity(content.len() + replacement.len());
    out.push_str(&content[..after_start]);
    out.push_str(replacement);
    out.push_str(&content[end_abs..]);
    Ok(out)
}

fn apply_regex(content: &str, pattern: &str, replacement: &str) -> Result<String, ActionError> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        return Err(ActionError::InvalidParams(format!(
            "regex pattern 超过 {MAX_REGEX_PATTERN_LEN} 字符"
        )));
    }
    let re = Regex::new(pattern)
        .map_err(|e| ActionError::InvalidParams(format!("无效 regex: {e}")))?;
    let out = re.replace_all(content, replacement).into_owned();
    if out.len() > MAX_REGEX_REPLACE_BYTES {
        return Err(ActionError::execution(format!(
            "regex 替换结果超过 {MAX_REGEX_REPLACE_BYTES} 字节"
        )));
    }
    Ok(out)
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
    let last = parts.pop().ok_or_else(|| ActionError::InvalidParams("空 pointer".into()))?;
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
        _ => Err(ActionError::execution(format!(
            "无法在路径 {path} 设置值"
        ))),
    }
}

fn write_result(path: PathBuf, changed: bool, bytes_written: usize) -> Value {
    let mut m = BTreeMap::new();
    m.insert("path".into(), Value::File(path));
    m.insert("changed".into(), Value::Bool(changed));
    m.insert("bytes_written".into(), Value::Int(bytes_written as i64));
    Value::Map(m)
}

pub struct FileRead;
pub struct FileWrite;
pub struct FileCopy;
pub struct FileDelete;

#[async_trait]
impl Action for FileRead {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.read",
            "File Read",
            "读取文本文件内容",
            ActionCategory::Data,
        )
        .with_params(vec![ParamSchema::new("path", SchemaType::File, true)])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        let text = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ActionError::execution(format!("读取文件失败 {}: {e}", path.display())))?;
        Ok(Value::Str(text))
    }
}

#[async_trait]
impl Action for FileWrite {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.write",
            "File Write",
            "写入或局部更新文本/JSON 文件",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("path", SchemaType::File, true),
            ParamSchema::new("content", SchemaType::Str, false),
            ParamSchema::new("mode", SchemaType::Str, false)
                .with_default("overwrite")
                .with_description("overwrite | replace_between | regex | json_set"),
            ParamSchema::new("start", SchemaType::Str, false),
            ParamSchema::new("end", SchemaType::Str, false),
            ParamSchema::new("pattern", SchemaType::Str, false),
            ParamSchema::new("replacement", SchemaType::Str, false),
            ParamSchema::new("pointer", SchemaType::Str, false),
            ParamSchema::new("value", SchemaType::Any, false),
            ParamSchema::new("create_dirs", SchemaType::Bool, false).with_default(true),
            ParamSchema::new("backup", SchemaType::Bool, false).with_default(false),
        ])
    }

    async fn execute(
        &self, params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let path = require_path(&params, "path")?;
        let path = confine_path(ctx, &path)?;
        let mode = opt_str(map, "mode").unwrap_or_else(|| "overwrite".into());
        let create_dirs = opt_bool(map, "create_dirs", true);
        let backup = opt_bool(map, "backup", false);

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

        let (final_content, changed) = match mode.as_str() {
            "overwrite" => {
                let content = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                (content.to_string(), true)
            }
            "replace_between" => {
                let start = require_str(map, "start")?;
                let end = require_str(map, "end")?;
                let replacement = map
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ActionError::MissingParam("content".into()))?;
                let out = replace_between(&existing, &start, &end, replacement)?;
                let changed = out != existing;
                (out, changed)
            }
            "regex" => {
                let pattern = require_str(map, "pattern")?;
                let replacement = opt_str(map, "replacement").unwrap_or_default();
                let out = apply_regex(&existing, &pattern, &replacement)?;
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
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的 file.write mode: {other}"
                )));
            }
        };

        let bytes = final_content.as_bytes();
        if changed || mode == "overwrite" {
            atomic_write(&path, bytes, backup).await?;
        }
        Ok(write_result(path, changed, bytes.len()))
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
        &self, params: Value,
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
impl Action for FileDelete {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "file.delete",
            "File Delete",
            "删除文件",
            ActionCategory::Data,
        )
        .with_params(vec![ParamSchema::new("path", SchemaType::File, true)])
    }

    async fn execute(
        &self, params: Value,
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
    registry.register(Arc::new(FileDelete));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;
    use tempfile::tempdir;

    #[tokio::test]
    async fn overwrite_and_replace_between() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let mut ctx = ExecutionContext::default();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("content".into(), Value::Str("<!--START-->old<!--END-->".into()));
        FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();

        let mut params = BTreeMap::new();
        params.insert("path".into(), Value::Str(path.display().to_string()));
        params.insert("mode".into(), Value::Str("replace_between".into()));
        params.insert("start".into(), Value::Str("<!--START-->".into()));
        params.insert("end".into(), Value::Str("<!--END-->".into()));
        params.insert("content".into(), Value::Str("new".into()));
        let out = FileWrite
            .execute(Value::Map(params), &mut ctx)
            .await
            .unwrap();
        assert!(out.as_map().unwrap().get("changed").unwrap().as_bool().unwrap());

        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(text, "<!--START-->new<!--END-->");
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
}
