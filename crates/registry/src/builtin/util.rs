//! 内置动作共用的参数辅助函数。

use corex_core::path::confine_in_roots;
use corex_core::{ActionError, ExecutionContext, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 设了 roots 时，拒绝 `ctx.config.filesystem_roots` 之外的路径。
pub fn confine_path(ctx: &ExecutionContext, path: &Path) -> Result<PathBuf, ActionError> {
    confine_in_roots(&ctx.config.filesystem_roots, path).map_err(|e| ActionError::execution(e.0))
}

pub fn require_map(params: &Value) -> Result<&BTreeMap<String, Value>, ActionError> {
    params
        .as_map()
        .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".to_string()))
}

pub fn require_str(map: &BTreeMap<String, Value>, key: &str) -> Result<String, ActionError> {
    map.get(key)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| ActionError::MissingParam(key.into()))
}

pub fn opt_str(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| v.as_str().map(|s| s.to_string()))
}

pub fn require_path(map: &BTreeMap<String, Value>, key: &str) -> Result<PathBuf, ActionError> {
    Ok(PathBuf::from(require_str(map, key)?))
}

pub fn opt_bool(map: &BTreeMap<String, Value>, key: &str, default: bool) -> bool {
    map.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

pub fn opt_i64(map: &BTreeMap<String, Value>, key: &str, default: i64) -> i64 {
    map.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

pub fn opt_f64(map: &BTreeMap<String, Value>, key: &str, default: f64) -> f64 {
    map.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
}

pub fn opt_strs(map: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    match map.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        Some(Value::Str(s)) => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn ensure_parent(path: &std::path::Path) -> Result<(), ActionError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// 递归统计 `path` 下的条目数（目录自身也算一个），用作删除类动作的进度总量。
///
/// 读不动的子目录按已知部分计入：这只是给人看的量级，不该让删除本身失败。
/// 不跟随符号链接 / junction，与 `remove_dir_all` 的语义一致。
pub fn count_entries(path: &Path) -> u64 {
    if !path.is_dir() {
        return 1;
    }
    let mut total = 1;
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            total += 1;
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(entry.path());
            }
        }
    }
    total
}
