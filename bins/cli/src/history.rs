//! `corex history`：读数据目录里那份只追加的执行历史。
//!
//! 历史从 v5 起就在写（`corex_engine::ExecutionHistory`），但此前只有 REPL 首屏在用，
//! 想查「昨天那条指令跑了多久」只能去 `cat` JSONL。这里把它变成一条命令，
//! 顺手把读取逻辑收成一份：`repl` 的「最近跑过」也走 [`recent`]。

use crate::output::{Role, outln, paint};
use crate::settings;
use anyhow::Result;
use corex_engine::HistoryEntry;
use corex_ipc::data_dir;
use std::path::{Path, PathBuf};

/// 读尾部这么多字节；要的是最后几条，不是整份账本。
const TAIL_BYTES: u64 = 128 * 1024;

/// 尾部最多扫这么多条：给 [`recent_names`] 留足去重的余地。
const SCAN: usize = 512;

/// 一次查询的条件。
pub(crate) struct Query {
    /// 只看这条指令。
    pub name: Option<String>,
    /// 只看失败的。
    pub is_failed_only: bool,
    /// 最多几条。
    pub limit: usize,
}

/// 列出最近几次执行；`-n` 之外没有别的花样。
pub(crate) fn run(query: Query) -> Result<()> {
    let entries = recent(query.limit)?;
    let mut shown = 0usize;
    for entry in &entries {
        if let Some(name) = &query.name
            && entry.directive != *name
        {
            continue;
        }
        if query.is_failed_only && entry.ok {
            continue;
        }
        outln!("{}", row(entry));
        shown += 1;
    }
    if shown == 0 {
        outln!("（没有匹配的执行记录）");
    }
    Ok(())
}

/// 一条记录：`2026-09-11 21:30:03  ✓ build-praise  13.25s`。
fn row(entry: &HistoryEntry) -> String {
    let (role, glyph) = if entry.ok {
        (Role::Ok, crate::output::symbols().ok)
    } else {
        (Role::Bad, crate::output::symbols().bad)
    };
    let line = format!(
        "{}  {} {:<20} {}",
        stamp(entry.started_at_ms),
        glyph,
        entry.directive,
        elapsed(entry.duration_ms)
    );
    match &entry.error {
        // 失败记录顺带把原因带上：再去翻 JSONL 才知道为什么失败就太绕了。
        Some(error) => format!("{line}  {error}"),
        None => paint(role, &line),
    }
}

/// unix 毫秒 → 本地时间 `YYYY-MM-DD HH:MM:SS`。
fn stamp(at_ms: u64) -> String {
    match chrono::DateTime::from_timestamp_millis(at_ms as i64) {
        Some(utc) => utc
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        // 超出可表示范围的时间戳只会出现在被人手改过的文件里。
        None => "???????????????????".to_string(),
    }
}

/// 耗时：不足一秒用毫秒，否则保留两位小数。
fn elapsed(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.2}s", ms as f64 / 1000.0)
    }
}

/// 最近 `limit` 条记录，新的排在前面。
///
/// 历史文件是按时间追加的，所以从尾部读即可——不必为了最后几条把整份读进内存。
/// 文件不存在或读不动就是空列表：还没跑过指令不是错误。
pub(crate) fn recent(limit: usize) -> Result<Vec<HistoryEntry>> {
    let mut entries = Vec::new();
    let Some(path) = log_path() else {
        return Ok(entries);
    };
    let Some(text) = tail(&path, TAIL_BYTES) else {
        return Ok(entries);
    };
    for line in text.lines().rev() {
        // 尾部第一行很可能是被截断的半个 JSON，解析失败跳过即可。
        let Ok(entry) = serde_json::from_str::<HistoryEntry>(line) else {
            continue;
        };
        entries.push(entry);
        if entries.len() == limit {
            break;
        }
    }
    Ok(entries)
}

/// 最近跑过的指令名，新的在前、同名只留一次。
///
/// REPL 首屏用它回答「这里有什么是刚跑过的」——读取逻辑与 `corex history` 共用。
pub(crate) fn recent_names(limit: usize) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for entry in recent(SCAN).unwrap_or_default() {
        if names.contains(&entry.directive) {
            continue;
        }
        names.push(entry.directive);
        if names.len() == limit {
            break;
        }
    }
    names
}

/// 生效的历史文件路径；历史被关掉时为 `None`。
fn log_path() -> Option<PathBuf> {
    let config = settings::effective();
    if !config.history.enabled {
        return None;
    }
    Some(if config.history.file.is_absolute() {
        config.history.file.clone()
    } else {
        data_dir().ok()?.join(&config.history.file)
    })
}

/// 文件末尾 `bytes` 个字节能读到的文本；读不动就是 `None`。
fn tail(path: &Path, bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len > bytes {
        // 起点可能落在多字节字符中间，所以按字节读、再宽松解码，而不是 `read_to_string`。
        file.seek(SeekFrom::Start(len - bytes)).ok()?;
    }
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}
