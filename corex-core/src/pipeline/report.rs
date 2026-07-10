use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::invoke::Artifact;

/// 单步执行报告
#[derive(Debug, Clone, Serialize)]
pub struct StepReport {
    pub id: String,
    pub module: String,
    pub status: StepStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    pub items: u64,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Success,
    Skipped,
    Failed,
}

/// Pipeline 执行报告
#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub pipeline_id: String,
    pub status: RunStatus,
    pub started_at: String,
    pub duration_ms: u64,
    pub steps: Vec<StepReport>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Success,
    Failed,
}

impl RunReport {
    pub fn new(pipeline_id: impl Into<String>) -> Self {
        Self {
            pipeline_id: pipeline_id.into(),
            status: RunStatus::Success,
            started_at: iso_now(),
            duration_ms: 0,
            steps: Vec::new(),
        }
    }

    pub fn fail(&mut self) {
        self.status = RunStatus::Failed;
    }
}

pub fn iso_now() -> String {
    use chrono::Utc;
    Utc::now().to_rfc3339()
}

pub fn write_report(path: &PathBuf, report: &RunReport) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, json)?;
    Ok(())
}

pub fn validate_errors_json(errors: &[String]) -> Value {
    serde_json::json!({
        "ok": false,
        "errors": errors,
    })
}
