use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

use crate::invoke::Artifact;

/// 单步失败（结构化错误，供 CLI 提取 step_id）
#[derive(Debug, Clone, Error)]
#[error("步骤 {step} 失败: {detail}")]
pub struct StepFail {
    pub step: String,
    pub detail: String,
}

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

    /// 首个失败步骤 `(step_id, detail)`
    pub fn first_fail(&self) -> Option<(&str, &str)> {
        self.steps.iter().find_map(|step| {
            if step.status == StepStatus::Failed {
                step.error.as_deref().map(|err| (step.id.as_str(), err))
            } else {
                None
            }
        })
    }

    /// CLI bail 用错误
    pub fn into_err(&self) -> anyhow::Error {
        if let Some((step, detail)) = self.first_fail() {
            StepFail {
                step: step.into(),
                detail: detail.into(),
            }
            .into()
        } else {
            anyhow::anyhow!("Pipeline 执行失败")
        }
    }

    /// CLI bail 用错误摘要（人类可读）
    pub fn message(&self) -> String {
        if let Some((id, err)) = self.first_fail() {
            format!("步骤 {id} 失败: {err}")
        } else {
            "Pipeline 执行失败".into()
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_includes_step_detail() {
        let mut report = RunReport::new("demo");
        report.fail();
        report.steps.push(StepReport {
            id: "copy_cache".into(),
            module: "copy".into(),
            status: StepStatus::Failed,
            artifact: None,
            items: 0,
            duration_ms: 1,
            error: Some("源路径不存在".into()),
        });
        assert_eq!(
            report.message(),
            "步骤 copy_cache 失败: 源路径不存在"
        );
    }
}

pub fn validate_errors_json(errors: &[String]) -> Value {
    serde_json::json!({
        "ok": false,
        "errors": errors,
    })
}
