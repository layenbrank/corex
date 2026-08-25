//! Shared param helpers for builtin actions.

use corex_core::{ActionError, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

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

pub fn opt_str_list(map: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    match map.get(key) {
        Some(Value::List(items)) => items
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
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}
