//! 数据目录下只追加的 JSONL 执行历史。

use corex_core::EngineError;
use serde::{Deserialize, Serialize};
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
    pub fn append(&self, entry: &HistoryEntry) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, entry).map_err(std::io::Error::other)?;
        file.write_all(b"\n")?;
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
}
