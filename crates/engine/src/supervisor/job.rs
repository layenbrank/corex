//! Supervisor job metadata on disk.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Supervisor job kind.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Watch,
    Cron,
}

/// Persisted job metadata under `<data>/<kind>/<id>/meta.json`.
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
}

impl JobMeta {
    pub fn job_dir(data_dir: &Path, kind: JobKind, id: &str) -> PathBuf {
        let sub = match kind {
            JobKind::Watch => "watch",
            JobKind::Cron => "cron",
        };
        data_dir.join(sub).join(id)
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
        serde_json::from_str(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
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

    /// Find a running supervisor job for the given directive name.
    pub fn find_running_by_directive(
        data_dir: &Path,
        kind: JobKind,
        directive_name: &str,
        is_pid_running: impl Fn(u32) -> bool,
    ) -> Option<Self> {
        Self::scan(data_dir, kind)
            .into_iter()
            .find(|j| j.directive_name == directive_name && is_pid_running(j.pid))
    }
}
