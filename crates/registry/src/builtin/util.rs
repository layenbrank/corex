//! 内置动作共用的参数辅助函数。

use corex_core::path::confine_in_roots;
use corex_core::{ActionError, ExecutionContext, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

/// 分块拷贝的进度落点：`done` 是当前文件已拷字节，`total` 是源文件大小（读不到时为 `None`）。
///
/// 传 `None` 表示没人看进度——那就走平台最优路径，别为一个没人看的百分比放弃它。
pub(crate) type Sink<'a> = &'a mut (dyn FnMut(u64, Option<u64>) + Send);

/// 分块复制的缓冲区：兼顾吞吐与上报粒度。
const COPY_CHUNK: usize = 1024 * 1024;

/// 复制单个文件，按需上报分块进度。
///
/// `sink` 为 `None` 时交给 `tokio::fs::copy`（`copy_file_range` / `CopyFileEx`）；有进度口时
/// 按 [`COPY_CHUNK`] 分块拷——大文件能给出中间进度就靠这一点，只数文件是不够的。
///
/// `file.copy` 与 `copy.run` 共用它：拷法只有一份，进度语义由调用方决定
/// （单文件报自己的字节，整棵树报累计字节）。
pub(crate) async fn copy_file(
    from: &Path,
    to: &Path,
    sink: Option<Sink<'_>>,
) -> Result<(), ActionError> {
    let Some(report) = sink else {
        tokio::fs::copy(from, to)
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        return Ok(());
    };

    let total = tokio::fs::metadata(from).await.map(|m| m.len()).ok();
    let mut src = tokio::fs::File::open(from)
        .await
        .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
    let mut dst = tokio::fs::File::create(to)
        .await
        .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
    let mut buf = vec![0u8; COPY_CHUNK];
    let mut done = 0u64;
    loop {
        let n = src
            .read(&mut buf)
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n])
            .await
            .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
        done += n as u64;
        report(done, total);
    }
    dst.flush()
        .await
        .map_err(|e| ActionError::execution(format!("复制失败: {e}")))?;
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
