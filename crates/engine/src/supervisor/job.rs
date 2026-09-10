//! 落盘的 supervisor 作业元数据。

use crate::supervisor::process::is_supervisor_alive;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// supervisor 作业类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Watch,
    Cron,
}

/// 持久化的作业元数据，位于 `<data>/<kind>/<id>/meta.json`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobMeta {
    pub id: String,
    pub kind: JobKind,
    pub directive_name: String,
    pub directive_path: PathBuf,
    pub pid: u32,
    #[serde(default)]
    pub expr: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    /// supervisor 所用 `corex` 二进制的绝对路径。
    #[serde(default)]
    pub supervisor_exe: Option<PathBuf>,
    /// 进程创建时间（unix 毫秒），用于识别 PID 复用。
    #[serde(default)]
    pub started_at_ms: Option<u64>,
}

impl JobMeta {
    pub fn job_dir(data_dir: &Path, kind: JobKind, id: &str) -> PathBuf {
        let sub = match kind {
            JobKind::Watch => "watch",
            JobKind::Cron => "cron",
        };
        data_dir.join(sub).join(id)
    }

    /// supervisor 进程日志文件的路径。
    pub fn supervisor_log_path(data_dir: &Path, kind: JobKind, id: &str) -> PathBuf {
        Self::job_dir(data_dir, kind, id).join("supervisor.log")
    }

    fn kind_label(kind: JobKind) -> &'static str {
        match kind {
            JobKind::Watch => "watch",
            JobKind::Cron => "cron",
        }
    }

    /// 记录的 supervisor 进程是否仍然活着。
    pub fn is_supervisor_alive(&self) -> bool {
        is_supervisor_alive(self)
    }

    /// 删除持久化的作业元数据（保留 `supervisor.log`）。
    pub fn remove(data_dir: &Path, kind: JobKind, id: &str) -> std::io::Result<()> {
        let dir = Self::job_dir(data_dir, kind, id);
        let _ = std::fs::remove_file(dir.join("meta.json"));
        let _ = std::fs::remove_file(dir.join("supervisor.pid"));
        let _ = std::fs::remove_file(dir.join("control.cmd"));
        Ok(())
    }

    /// 按指令名解析作业（不是按操作系统 pid）。
    pub fn resolve_by_name(data_dir: &Path, kind: JobKind, name: &str) -> Result<Self, String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("指令名不能为空".into());
        }
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!(
                "请使用指令名，非 pid。可用 corex {} ps 查看 NAME 列",
                Self::kind_label(kind)
            ));
        }
        Self::read(data_dir, kind, trimmed).map_err(|_| {
            let scanned = Self::scan(data_dir, kind);
            let names: Vec<String> = scanned.iter().map(|j| j.directive_name.clone()).collect();
            if names.is_empty() {
                format!(
                    "未找到 {} job `{trimmed}`（当前无运行中的 job）",
                    Self::kind_label(kind)
                )
            } else {
                format!(
                    "未找到 {} job `{trimmed}`。可用: {}",
                    Self::kind_label(kind),
                    names.join(", ")
                )
            }
        })
    }

    pub fn write(&self, data_dir: &Path) -> std::io::Result<()> {
        let dir = Self::job_dir(data_dir, self.kind, &self.id);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("meta.json"), serde_json::to_string_pretty(self)?)?;
        std::fs::write(dir.join("supervisor.pid"), self.pid.to_string())?;
        Ok(())
    }

    pub fn read(data_dir: &Path, kind: JobKind, id: &str) -> std::io::Result<Self> {
        let path = Self::job_dir(data_dir, kind, id).join("meta.json");
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    pub fn scan(data_dir: &Path, kind: JobKind) -> Vec<Self> {
        let root = JobMeta::job_dir(data_dir, kind, "");
        let parent = root.parent().unwrap_or(data_dir);
        let sub = match kind {
            JobKind::Watch => "watch",
            JobKind::Cron => "cron",
        };
        let base = parent.join(sub);
        let Ok(entries) = std::fs::read_dir(&base) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if let Ok(meta) = Self::read(data_dir, kind, &id) {
                out.push(meta);
            }
        }
        out
    }

    /// 清理那些已经不再指向存活 supervisor 的作废记录。
    pub fn prune_stale(data_dir: &Path, kind: JobKind) {
        for meta in Self::scan(data_dir, kind) {
            if !meta.is_supervisor_alive() {
                let _ = Self::remove(data_dir, kind, &meta.id);
            }
        }
    }

    /// 找出该指令名下正在运行的 supervisor 作业。
    pub fn find_running_by_directive(
        data_dir: &Path,
        kind: JobKind,
        directive_name: &str,
    ) -> Option<Self> {
        Self::scan(data_dir, kind)
            .into_iter()
            .find(|j| j.directive_name == directive_name && j.is_supervisor_alive())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_stale_removes_legacy_meta() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        let meta = JobMeta {
            id: "demo".into(),
            kind: JobKind::Watch,
            directive_name: "demo".into(),
            directive_path: PathBuf::from("demo.yaml"),
            pid: 999_999,
            expr: None,
            paths: vec![],
            supervisor_exe: None,
            started_at_ms: None,
        };
        meta.write(data).unwrap();
        assert!(JobMeta::read(data, JobKind::Watch, "demo").is_ok());
        JobMeta::prune_stale(data, JobKind::Watch);
        assert!(JobMeta::read(data, JobKind::Watch, "demo").is_err());
    }
}
