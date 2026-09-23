//! `corex history`：读数据目录里那份只追加的执行历史。
//!
//! 历史从 v5 起就在写（`corex_engine::ExecutionHistory`），但此前只有 REPL 首屏在用，
//! 想查「昨天那条指令跑了多久」只能去 `cat` JSONL。这里把它变成一条命令；读取逻辑则全在
//! 引擎里（`recent` / `recent_names` / `by_directive`），daemon 与宿主读的是同一份实现，
//! CLI 只负责回答「读哪个文件」。

use crate::output::{Role, outln, paint};
use crate::settings;
use anyhow::Result;
use corex_engine::{ExecutionHistory, HistoryEntry};
use corex_ipc::data_dir;

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

/// 最近几次执行，新的在前；`name` 与 `limit` 都交给引擎，CLI 不再自己扫文件。
///
/// 打不开（历史被关掉 / 文件读不动）就是空列表：还没跑过指令不是错误。
pub(crate) fn recent(name: Option<&str>, limit: usize) -> Vec<HistoryEntry> {
    open()
        .map(|history| history.recent(name, limit))
        .unwrap_or_default()
}

/// 最近跑过的指令名，新的在前、同名只留一次。
///
/// REPL 首屏用它回答「这里有什么是刚跑过的」——读取逻辑与 `corex history` 共用。
pub(crate) fn recent_names(limit: usize) -> Vec<String> {
    open()
        .map(|history| history.recent_names(limit))
        .unwrap_or_default()
}

/// 生效的那份历史；历史被关掉、或文件开不了时为 `None`。
///
/// 路径解析留在 CLI：`data_dir()` 与 `corex.toml` 是**进程自己**看到的那些，交给 daemon
/// 猜就会与用户敲下这条命令时的心智不一致。
fn open() -> Option<ExecutionHistory> {
    let config = settings::effective();
    if !config.history.enabled {
        return None;
    }
    let path = if config.history.file.is_absolute() {
        config.history.file.clone()
    } else {
        data_dir().ok()?.join(&config.history.file)
    };
    ExecutionHistory::open(path).ok()
}
