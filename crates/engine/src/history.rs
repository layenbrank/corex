//! 数据目录下只追加的 JSONL 执行历史。

use corex_core::EngineError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// 一条指令 / 流水线执行记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    /// 指令名（或文件名主干）。
    pub directive: String,
    /// 执行开始的 unix 毫秒时间戳。
    pub started_at_ms: u64,
    /// 执行结束的 unix 毫秒时间戳。
    pub ended_at_ms: u64,
    /// 执行是否成功。
    pub ok: bool,
    /// `ok` 为 false 时的错误消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 实际耗时（毫秒）。
    pub duration_ms: u64,
}

impl HistoryEntry {
    pub fn new(
        directive: impl Into<String>,
        started: SystemTime,
        ended: SystemTime,
        result: Result<(), &EngineError>,
    ) -> Self {
        let started_at_ms = system_time_ms(started);
        let ended_at_ms = system_time_ms(ended);
        let duration_ms = ended
            .duration_since(started)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64;
        let (ok, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(sanitize_history_error(e))),
        };
        Self {
            directive: directive.into(),
            started_at_ms,
            ended_at_ms,
            ok,
            error,
            duration_ms,
        }
    }
}

/// 一条指令的近期表现：**最近一次**跑成什么样 + 窗口内跑了多少次。
///
/// 卡片上的「上次执行时间 / 上次成功没」不该由宿主自己攒：历史是引擎在跑完的当口写的，
/// 宿主再存一份必然与它对不上（换台机器、清过数据目录都会露馅）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectiveHistory {
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub ok: bool,
    /// `ok` 为 false 时的错误消息（与历史文件里一样已脱敏、截断）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
    /// 窗口内这条指令跑了几次（含失败）。
    pub run_count: usize,
    /// 窗口内失败了几次。
    pub failed_count: usize,
}

impl DirectiveHistory {
    /// 拿一条执行记录当「最近一次」，计数从 0 起（由聚合方累加）。
    fn from_run(entry: &HistoryEntry) -> Self {
        Self {
            started_at_ms: entry.started_at_ms,
            ended_at_ms: entry.ended_at_ms,
            ok: entry.ok,
            error: entry.error.clone(),
            duration_ms: entry.duration_ms,
            run_count: 0,
            failed_count: 0,
        }
    }
}

/// 历史错误文本的最大长度（路径脱敏之后）。
const HISTORY_ERROR_MAX: usize = 200;

/// 从 [`EngineError`] 归类 + 路径脱敏 + 截断。
/// 完整细节仍保留在 `audit.jsonl` / 进程日志里。
pub fn sanitize_history_error(err: &EngineError) -> String {
    let kind = err.kind();
    let redacted = redact_path_like(&err.to_string());
    let body = truncate_chars(&redacted, HISTORY_ERROR_MAX);
    if body.is_empty() {
        kind
    } else {
        format!("{kind}: {body}")
    }
}

fn redact_path_like(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    for token in msg.split_whitespace() {
        if looks_like_path(token) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str("<path>");
        } else {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(token);
        }
    }
    out
}

fn looks_like_path(token: &str) -> bool {
    let t = token.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == ',' || c == ';');
    if t.len() < 3 {
        return false;
    }
    // Unix 绝对路径，或 Windows 盘符 / 类 UNC
    t.starts_with('/')
        || t.starts_with('\\')
        || (t.len() >= 3
            && t.as_bytes()[1] == b':'
            && (t.as_bytes()[2] == b'\\' || t.as_bytes()[2] == b'/'))
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max).collect();
    format!("{truncated}…")
}

fn system_time_ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

/// 读尾部这么多字节：要的是最后几条，不是整份账本。
///
/// 一次运行就可能写上百行，几 MB 之后没人愿意为了第一屏把整份解析一遍。
const TAIL_BYTES: u64 = 128 * 1024;

/// 聚合 / 去重时最多回看多少条：给 [`ExecutionHistory::recent_names`] 留足去重的余地。
const SCAN: usize = 512;

/// 执行历史的只追加 JSONL 写入器。
#[derive(Debug, Clone)]
pub struct ExecutionHistory {
    path: PathBuf,
}

impl ExecutionHistory {
    /// 在指定路径打开（或创建）历史文件。
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // 先碰一下文件，让父目录缺失 / 权限不足尽早暴露。
        let _ = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { path })
    }

    /// `data_dir` 下的默认文件（`history.jsonl`）。
    pub fn under_data_dir(data_dir: &Path) -> std::io::Result<Self> {
        Self::open(data_dir.join("history.jsonl"))
    }

    /// 历史文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 追一条记录，占一行 JSON。
    ///
    /// 整行**一次 `write_all`** 写完：写历史的不止一个进程（`corex run` 与 daemon 各写
    /// 一份），进程内的锁覆盖不到跨进程；分次写时两边的字节会互相插进对方行里，而
    /// [`Self::read_all`] 只能把插坏的整行丢掉。单次写配 `O_APPEND` 才是原子的。
    pub fn append(&self, entry: &HistoryEntry) -> std::io::Result<()> {
        let mut line = serde_json::to_vec(entry).map_err(std::io::Error::other)?;
        line.push(b'\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&line)?;
        file.flush()?;
        debug!(
            path = %self.path.display(),
            directive = %entry.directive,
            ok = entry.ok,
            duration_ms = entry.duration_ms,
            "已写入执行历史"
        );
        Ok(())
    }

    /// 追加；失败只记警告，不向上传播。
    pub fn record_best_effort(&self, entry: &HistoryEntry) {
        if let Err(e) = self.append(entry) {
            warn!(
                path = %self.path.display(),
                error = %e,
                "写入执行历史失败"
            );
        }
    }

    /// 读出全部记录（测试 / 排查用）。跳过格式坏掉的行。
    pub fn read_all(&self) -> std::io::Result<Vec<HistoryEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&self.path)?;
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<HistoryEntry>(line) {
                Ok(e) => out.push(e),
                Err(err) => warn!(error = %err, "跳过损坏的历史行"),
            }
        }
        Ok(out)
    }

    /// 最近的执行记录，新的在前；`name` 只看一条指令，`limit` 最多回几条。
    ///
    /// 只读文件**尾巴**（[`TAIL_BYTES`]）：历史是只追加的，为了第一屏把整份账本解析一遍
    /// 不值得。追加写到一半的那行解析不了，跳过即可。
    pub fn recent(&self, name: Option<&str>, limit: usize) -> Vec<HistoryEntry> {
        if limit == 0 {
            return Vec::new();
        }
        let Some(text) = self.tail(TAIL_BYTES) else {
            return Vec::new();
        };
        text.lines()
            .rev()
            .filter_map(|line| serde_json::from_str::<HistoryEntry>(line).ok())
            // 先过滤再截断：`--name` 要 3 条时，回的不该是「最近 3 条里恰好匹配的那几条」。
            .filter(|entry| name.is_none_or(|name| entry.directive == name))
            .take(limit)
            .collect()
    }

    /// 最近跑过的指令名，新的在前、同名只留一次。
    ///
    /// REPL 首屏用它回答「这里有什么是刚跑过的」——读取逻辑与 `corex history` 共用。
    pub fn recent_names(&self, limit: usize) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for entry in self.recent(None, SCAN) {
            if names.iter().any(|seen| *seen == entry.directive) {
                continue;
            }
            names.push(entry.directive);
            if names.len() == limit {
                break;
            }
        }
        names
    }

    /// 按指令名聚合「最近一次 + 窗口内的次数」。
    ///
    /// 记录是追加写的，所以倒着扫：第一次见到某条指令的那条就是它最近的一次。
    pub fn by_directive(&self) -> BTreeMap<String, DirectiveHistory> {
        let mut out: BTreeMap<String, DirectiveHistory> = BTreeMap::new();
        for entry in self.recent(None, SCAN) {
            let summary = out
                .entry(entry.directive.clone())
                .or_insert_with(|| DirectiveHistory::from_run(&entry));
            summary.run_count += 1;
            if !entry.ok {
                summary.failed_count += 1;
            }
        }
        out
    }

    /// 文件末尾 `bytes` 个字节能读到的文本；读不动就是 `None`。
    fn tail(&self, bytes: u64) -> Option<String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = std::fs::File::open(&self.path).ok()?;
        let len = file.metadata().ok()?.len();
        if len > bytes {
            // 起点可能落在多字节字符中间，所以按字节读、再宽松解码，而不是 `read_to_string`。
            file.seek(SeekFrom::Start(len - bytes)).ok()?;
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).ok()?;
        Some(String::from_utf8_lossy(&buf).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ActionError;
    use std::time::SystemTime;

    #[test]
    fn append_and_read_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let hist = ExecutionHistory::under_data_dir(dir.path()).unwrap();
        let start = SystemTime::now();
        let end = start + Duration::from_millis(12);
        let entry = HistoryEntry::new("hello", start, end, Ok(()));
        hist.append(&entry).unwrap();
        let boom = EngineError::other("boom");
        hist.append(&HistoryEntry::new("fail", start, end, Err(&boom)))
            .unwrap();

        let all = hist.read_all().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].ok);
        assert_eq!(all[0].directive, "hello");
        assert!(!all[1].ok);
        assert_eq!(all[1].error.as_deref(), Some("execution: boom"));
        assert_eq!(all[0].duration_ms, 12);
    }

    /// 并发追加不许互相插行：`max_jobs` 提到 4 之后，同一个进程里就有多条流水线同时写。
    ///
    /// 分次写（先写 JSON 再写换行）时两个线程的字节会交错，插坏的整行会被 `read_all`
    /// 静默丢掉——表现为「跑过但账本里没有」。这里量条数，少了就是丢了。
    #[test]
    fn concurrent_appends_keep_every_line() {
        let dir = tempfile::tempdir().unwrap();
        let hist = ExecutionHistory::under_data_dir(dir.path()).unwrap();
        let start = SystemTime::now();
        let threads: Vec<_> = (0..4)
            .map(|writer| {
                let hist = hist.clone();
                std::thread::spawn(move || {
                    for n in 0..25 {
                        let name = format!("d{writer}-{n}");
                        hist.append(&HistoryEntry::new(
                            &name,
                            start,
                            start + Duration::from_millis(1),
                            Ok(()),
                        ))
                        .unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }

        let all = hist.read_all().unwrap();
        assert_eq!(all.len(), 100, "并发追加丢了记录");
        let names: std::collections::BTreeSet<_> =
            all.iter().map(|entry| entry.directive.as_str()).collect();
        assert_eq!(names.len(), 100, "记录被插行插坏了: {names:?}");
    }

    #[test]
    fn sanitize_redacts_paths_and_classifies() {
        let err = EngineError::other("failed reading /tmp/secret.txt under root");
        let s = sanitize_history_error(&err);
        assert!(s.starts_with("execution:"), "{s}");
        assert!(s.contains("<path>"), "{s}");
        assert!(!s.contains("/tmp/secret.txt"), "{s}");

        let perm = EngineError::Action(ActionError::PermissionDenied("strict_permissions".into()));
        let s = sanitize_history_error(&perm);
        assert!(s.starts_with("permission_denied:"), "{s}");
    }

    /// 三次运行，两条指令：顺序、过滤、条数一次验完。
    #[test]
    fn recent_reads_newest_first_and_filters_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let hist = ExecutionHistory::under_data_dir(dir.path()).unwrap();
        let start = SystemTime::now();
        let boom = EngineError::other("boom");
        for (name, offset, result) in [
            ("a", 1u64, Ok(())),
            ("b", 2, Err(&boom)),
            ("a", 3, Ok(())),
            ("c", 4, Ok(())),
        ] {
            hist.append(&HistoryEntry::new(
                name,
                start + Duration::from_millis(offset * 10),
                start + Duration::from_millis(offset * 10 + 5),
                result,
            ))
            .unwrap();
        }

        let recent = hist.recent(None, 3);
        let names: Vec<&str> = recent.iter().map(|e| e.directive.as_str()).collect();
        assert_eq!(names, ["c", "a", "b"], "新的在前");

        assert_eq!(hist.recent(Some("a"), 10).len(), 2);
        assert!(
            hist.recent(Some("a"), 10)
                .iter()
                .all(|e| e.directive == "a")
        );
        assert_eq!(
            hist.recent(Some("a"), 1).len(),
            1,
            "条数上限要在按名过滤之后算"
        );
        assert!(hist.recent(Some("a"), 0).is_empty());

        // 追加写到一半的那半行不该让整份历史读不出来。
        let mut file = OpenOptions::new().append(true).open(hist.path()).unwrap();
        file.write_all(b"{\"directive\":\"trunca").unwrap();
        assert_eq!(hist.recent(None, 1).len(), 1, "坏行跳过，前一条仍在");
    }

    /// 卡片要的两件事：最近一次 + 窗口内跑了多少次（失败也算进去）。
    #[test]
    fn by_directive_keeps_the_newest_run_and_counts_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let hist = ExecutionHistory::under_data_dir(dir.path()).unwrap();
        let start = SystemTime::now();
        let boom = EngineError::other("boom");
        let entries = [
            ("a", 1u64, Ok(())),
            ("b", 2, Err(&boom)),
            ("a", 3, Err(&boom)),
        ];
        for (name, offset, result) in entries {
            hist.append(&HistoryEntry::new(
                name,
                start + Duration::from_millis(offset * 10),
                start + Duration::from_millis(offset * 10 + 7),
                result,
            ))
            .unwrap();
        }

        let map = hist.by_directive();
        let a = map.get("a").expect("a 有两次运行");
        assert!(!a.ok, "最近一次是失败的那次");
        assert_eq!(a.run_count, 2);
        assert_eq!(a.failed_count, 1);
        assert_eq!(a.duration_ms, 7);
        assert_eq!(
            a.started_at_ms,
            system_time_ms(start + Duration::from_millis(30))
        );
        assert!(a.error.is_some());

        let b = map.get("b").expect("b 有一次运行");
        assert_eq!(b.run_count, 1);
        assert_eq!(b.failed_count, 1);
        assert!(!b.ok);

        let names = hist.recent_names(2);
        assert_eq!(names, ["a", "b"], "「最近跑过」按时间倒序、同名只留一次");
    }
}
