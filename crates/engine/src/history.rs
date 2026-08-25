//! Append-only JSONL execution history under the data directory.

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// One shortcut / pipeline execution record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Shortcut name (or file stem).
    pub shortcut: String,
    /// Unix epoch millis when execution started.
    pub started_at_ms: u64,
    /// Unix epoch millis when execution ended.
    pub ended_at_ms: u64,
    /// Whether execution succeeded.
    pub ok: bool,
    /// Error message when `ok` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Wall duration in milliseconds.
    pub duration_ms: u64,
}

impl HistoryEntry {
    pub fn new(
        shortcut: impl Into<String>,
        started: SystemTime,
        ended: SystemTime,
        result: Result<(), String>,
    ) -> Self {
        let started_at_ms = system_time_ms(started);
        let ended_at_ms = system_time_ms(ended);
        let duration_ms = ended
            .duration_since(started)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64;
        let (ok, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };
        Self {
            shortcut: shortcut.into(),
            started_at_ms,
            ended_at_ms,
            ok,
            error,
            duration_ms,
        }
    }
}

fn system_time_ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

/// Append-only JSONL writer for execution history.
#[derive(Debug, Clone)]
pub struct ExecutionHistory {
    path: PathBuf,
}

impl ExecutionHistory {
    /// Open (or create) history at an explicit path.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Touch the file so missing parents / permissions fail early.
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        Ok(Self { path })
    }

    /// Default file under `data_dir` (`history.jsonl`).
    pub fn under_data_dir(data_dir: &Path) -> std::io::Result<Self> {
        Self::open(data_dir.join("history.jsonl"))
    }

    /// History file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one entry as a single JSON line.
    pub fn append(&self, entry: &HistoryEntry) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, entry).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e)
        })?;
        file.write_all(b"\n")?;
        file.flush()?;
        debug!(
            path = %self.path.display(),
            shortcut = %entry.shortcut,
            ok = entry.ok,
            duration_ms = entry.duration_ms,
            "已写入执行历史"
        );
        Ok(())
    }

    /// Append, logging a warning on failure instead of propagating.
    pub fn record_best_effort(&self, entry: &HistoryEntry) {
        if let Err(e) = self.append(entry) {
            warn!(
                path = %self.path.display(),
                error = %e,
                "写入执行历史失败"
            );
        }
    }

    /// Read all entries (for tests / inspection). Skips malformed lines.
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
    use std::time::SystemTime;

    #[test]
    fn append_and_read_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let hist = ExecutionHistory::under_data_dir(dir.path()).unwrap();
        let start = SystemTime::now();
        let end = start + Duration::from_millis(12);
        let entry = HistoryEntry::new("hello", start, end, Ok(()));
        hist.append(&entry).unwrap();
        hist.append(&HistoryEntry::new(
            "fail",
            start,
            end,
            Err("boom".into()),
        ))
        .unwrap();

        let all = hist.read_all().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].ok);
        assert_eq!(all[0].shortcut, "hello");
        assert!(!all[1].ok);
        assert_eq!(all[1].error.as_deref(), Some("boom"));
        assert_eq!(all[0].duration_ms, 12);
    }
}
