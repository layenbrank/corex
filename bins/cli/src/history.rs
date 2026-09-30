//! `corex history`：读指令库里的执行日志。
//!
//! 历史从 v5 起就在写，但落点换过一次：v12 及以前是一个只追加的 JSONL 文件，v13 起与指令
//! 同库（`<数据目录>/directives.db` 的 `runs` 表）——「上次执行时间」这类记录必须与指令放在
//! 一起，否则宿主显示的时间与账本会各说各话。读取逻辑仍在引擎/库里（`recent` /
//! `recent_names` / `by_directive`），CLI 只负责把它打出来。

use crate::library::Library;
use crate::output::{Role, outln, paint};
use anyhow::Result;
use corex_engine::{HistoryEntry, HistorySink};
use std::sync::Arc;

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
    let entries = recent(query.name.as_deref(), query.limit);
    let mut shown = 0usize;
    for entry in &entries {
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

/// 最近几次执行，新的在前；`name` 与 `limit` 都交给库里那份账本，CLI 不再自己扫文件。
///
/// 打不开（历史被关掉 / 库读不动）就是空列表：还没跑过指令不是错误。
pub(crate) fn recent(name: Option<&str>, limit: usize) -> Vec<HistoryEntry> {
    sink()
        .map(|history| history.recent(name, limit))
        .unwrap_or_default()
}

/// 最近跑过的指令名，新的在前、同名只留一次。
///
/// REPL 首屏用它回答「这里有什么是刚跑过的」——读取逻辑与 `corex history` 共用。
pub(crate) fn recent_names(limit: usize) -> Vec<String> {
    sink()
        .map(|history| history.recent_names(limit))
        .unwrap_or_default()
}

/// 生效的那份账本；`[history] enabled = false` 时是 `None`。
///
/// 打开指令库这件事本身失败（数据目录不可写、库损坏）也当「没有历史」：`history` 是只读的
/// 辅助命令，为它让整条命令失败不值当——真正的失败会在 `run` / `directive` 那几条命令上暴露。
fn sink() -> Option<Arc<dyn HistorySink>> {
    Library::open().ok()?.history()
}
