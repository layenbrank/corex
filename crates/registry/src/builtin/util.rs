//! 内置动作共用的参数辅助函数。

use corex_core::path::confine_in_roots;
use corex_core::{ActionError, ExecutionContext, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// 分块拷贝是三处拷贝动作共用的实现（`copy.run` / `file.copy` / `morph.export`），
// 因此跟着它们一起 gate：一个 `act-*` 都不开时，这里不该剩下几个没人用的函数。
#[cfg(any(feature = "act-copy", feature = "act-file", feature = "act-morph"))]
use corex_core::Unit;
#[cfg(any(feature = "act-copy", feature = "act-file", feature = "act-morph"))]
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
#[cfg(any(feature = "act-copy", feature = "act-file", feature = "act-morph"))]
type Sink<'a> = &'a mut (dyn FnMut(u64, Option<u64>) + Send);

/// 分块拷贝的缓冲区：兼顾吞吐与上报粒度。
#[cfg(any(feature = "act-copy", feature = "act-file", feature = "act-morph"))]
const COPY_CHUNK: usize = 1024 * 1024;

/// 复制单个文件，按需上报分块进度。
///
/// `sink` 为 `None` 时交给 `tokio::fs::copy`（`copy_file_range` / `CopyFileEx`）；有进度口时
/// 按 [`COPY_CHUNK`] 分块拷——大文件能给出中间进度就靠这一点，只数文件是不够的。
///
/// 进度语义（算多少、怎么算）由调用方决定，见 [`copy_bytes`]。
#[cfg(any(feature = "act-copy", feature = "act-file", feature = "act-morph"))]
async fn copy_file(from: &Path, to: &Path, sink: Option<Sink<'_>>) -> Result<(), ActionError> {
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

/// 拷一个文件，把进度报在**整批**的字节量上。
///
/// `offset` 是本批已完成的字节，`total` 是本批总量：单文件动作用 `0` 与文件大小，
/// 整棵树用累加值——这样界面上的百分比始终是「这整件事干到哪儿了」。
///
/// 没有上报口就不挂口子，[`copy_file`] 会改走平台最优路径。
#[cfg(any(feature = "act-copy", feature = "act-file", feature = "act-morph"))]
pub(crate) async fn copy_bytes(
    from: &Path,
    to: &Path,
    offset: u64,
    total: u64,
    ctx: &ExecutionContext,
) -> Result<(), ActionError> {
    let mut report = |done: u64, _file_total: Option<u64>| {
        ctx.chunk(offset + done, Some(total), Unit::Bytes);
    };
    let sink = ctx.observer.is_some().then_some(&mut report as Sink);
    copy_file(from, to, sink).await
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

/// 动作单测用的进度记录器。
///
/// 上报口在动作里是「有就报、没有就算了」，于是断言只能落在**报了什么**上：
/// 这里把某一单位的帧按顺序记下来，供各动作的单测复用。
#[cfg(test)]
pub(crate) mod probe {
    use corex_core::{Mark, Observer, Spot, Unit};
    use std::sync::{Arc, Mutex};

    /// 只收指定单位的帧；`begin` / `end` 一概不关心。
    #[derive(Debug)]
    pub(crate) struct Probe {
        unit: Unit,
        marks: Mutex<Vec<(u64, Option<u64>)>>,
    }

    impl Probe {
        /// 收字节帧。
        pub(crate) fn bytes() -> Arc<Self> {
            Self::of(Unit::Bytes)
        }

        /// 收条目帧。
        pub(crate) fn items() -> Arc<Self> {
            Self::of(Unit::Items)
        }

        fn of(unit: Unit) -> Arc<Self> {
            Arc::new(Self {
                unit,
                marks: Mutex::new(Vec::new()),
            })
        }

        /// 收到的帧，按发生顺序。
        pub(crate) fn marks(&self) -> Vec<(u64, Option<u64>)> {
            self.marks.lock().unwrap().clone()
        }
    }

    impl Observer for Probe {
        fn chunk(&self, _at: Spot<'_>, mark: Mark) {
            if mark.unit == self.unit {
                self.marks.lock().unwrap().push((mark.done, mark.total));
            }
        }
    }
}
