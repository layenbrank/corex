//! 内置动作共用的参数辅助函数。

use corex_core::path::confine_in_roots;
use corex_core::{ActionError, ExecutionContext, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// 拷贝与区间读取共用 tokio 的 io 扩展。`AsyncReadExt` 两侧都要，所以单独一行、
// 匿名导入（`as _` 只把方法带进作用域，不会再引入一个名字）。
#[cfg(any(
    feature = "act-copy",
    feature = "act-file",
    feature = "act-generate",
    feature = "act-http",
    feature = "act-morph"
))]
use tokio::io::AsyncReadExt as _;
#[cfg(any(feature = "act-file", feature = "act-generate", feature = "act-http"))]
use tokio::io::AsyncSeekExt;
#[cfg(any(feature = "act-copy", feature = "act-file", feature = "act-morph"))]
use tokio::io::AsyncWriteExt;

// 分块拷贝与区间读取都是跟着它们的调用方一起 gate 的：
// 一个 `act-*` 都不开时，这里不该剩下几个没人用的符号。
#[cfg(any(feature = "act-copy", feature = "act-file", feature = "act-morph"))]
use corex_core::Unit;

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

/// `offset` / `length` 这一对参数：不填 `length` 就是读到结尾（负数直接报错）。
///
/// 摘要、分片计划、multipart 部件与 `file.read` 的 bytes 模式都用这一套读法，
/// 因此「一段」的边界只有这里在解释。
#[cfg(any(feature = "act-file", feature = "act-generate", feature = "act-http"))]
pub(crate) fn range_params(
    map: &BTreeMap<String, Value>,
) -> Result<(u64, Option<u64>), ActionError> {
    let offset = map.get("offset").and_then(|v| v.as_i64()).unwrap_or(0);
    if offset < 0 {
        return Err(ActionError::InvalidParams("offset 不能为负".into()));
    }
    let length = match map.get("length").and_then(|v| v.as_i64()) {
        Some(n) if n < 0 => return Err(ActionError::InvalidParams("length 不能为负".into())),
        Some(n) => Some(n as u64),
        None => None,
    };
    Ok((offset as u64, length))
}

/// 单个缓冲区允许的字节上限。
///
/// 「一段」是整块读进内存的（分片计划要算摘要、multipart 要原样发出去）：
/// 这是它们的共同天花板，也是「别把 10 GB 一次读进来」这条规矩的唯一出处。
#[cfg(any(feature = "act-file", feature = "act-generate", feature = "act-http"))]
pub(crate) const MAX_RANGE: u64 = 256 * 1024 * 1024;

/// 文件里的一段：起点 + 还剩多少字节。
///
/// 摘要与分片计划按块从这里读，因此文件多大都只占一个缓冲区。
#[cfg(any(feature = "act-file", feature = "act-generate", feature = "act-http"))]
pub(crate) struct Slice {
    file: tokio::fs::File,
    left: u64,
}

#[cfg(any(feature = "act-file", feature = "act-generate", feature = "act-http"))]
impl Slice {
    /// 打开 `path` 的 `[offset, offset + length)`；`length` 为 `None` 时读到结尾。
    pub(crate) async fn open(
        path: &Path,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Self, ActionError> {
        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| ActionError::execution(format!("读取文件失败 {}: {e}", path.display())))?;
        if !meta.is_file() {
            return Err(ActionError::InvalidParams(format!(
                "不是文件: {}",
                path.display()
            )));
        }
        let size = meta.len();
        if offset > size {
            return Err(ActionError::InvalidParams(format!(
                "offset {offset} 超过文件大小 {size}"
            )));
        }
        let left = length.map_or(size - offset, |n| n.min(size - offset));

        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| ActionError::execution(format!("打开文件失败 {}: {e}", path.display())))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|e| ActionError::execution(format!("定位失败 {}: {e}", path.display())))?;
        }
        Ok(Self { file, left })
    }

    /// 这段还剩多少字节。
    pub(crate) fn len(&self) -> u64 {
        self.left
    }

    /// 读一段，最多 `buf.len()` 字节；返回 0 表示这段读完了。
    pub(crate) async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ActionError> {
        if self.left == 0 {
            return Ok(0);
        }
        let want = buf.len().min(self.left as usize);
        let n = self
            .file
            .read(&mut buf[..want])
            .await
            .map_err(|e| ActionError::execution(format!("读取失败: {e}")))?;
        self.left -= n as u64;
        Ok(n)
    }
}

/// 把 `path` 的 `[offset, offset + length)` 整个读进内存，最多 `max` 字节。
///
/// 适合「一片」这种粒度（multipart 部件、`file.read` 的 bytes 模式）；
/// 再大就该用 [`Slice`] 边读边处理了。
#[cfg(any(feature = "act-file", feature = "act-generate", feature = "act-http"))]
pub(crate) async fn read_range(
    path: &Path,
    offset: u64,
    length: Option<u64>,
    max: u64,
) -> Result<Vec<u8>, ActionError> {
    let mut slice = Slice::open(path, offset, length).await?;
    let size = slice.len();
    if size > max {
        return Err(ActionError::InvalidParams(format!(
            "一次读取 {size} 字节，超过 {max} 上限"
        )));
    }
    let mut out = vec![0u8; size as usize];
    let mut filled = 0usize;
    while filled < out.len() {
        let n = slice.read(&mut out[filled..]).await?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    out.truncate(filled);
    Ok(out)
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
