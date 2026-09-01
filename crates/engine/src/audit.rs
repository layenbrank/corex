//! Step-level audit log (redacted; no bodies / OCR / clipboard content).

use corex_core::{ActionError, EngineError};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// One step execution audit record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEntry {
    /// Directive (or Directive) name.
    pub name: String,
    pub step_id: String,
    pub action_id: String,
    pub duration_ms: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    /// `true` when the step was rejected by permission / policy checks.
    #[serde(default = "default_denied")]
    pub denied: bool,
    /// UI automation phase hint (launch/login/act/verify).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_phase: Option<String>,
    /// Structured error code (e.g. ui_login_pending).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Redacted selector hint (no PII); from [`ActionError::selector_hint`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector_hint: Option<String>,
}

impl AuditEntry {
    /// Build from a typed [`EngineError`] (pipeline steps).
    pub fn from_engine(
        name: impl Into<String>,
        step_id: impl Into<String>,
        action_id: impl Into<String>,
        duration_ms: u64,
        result: Result<(), &EngineError>,
    ) -> Self {
        match result {
            Ok(()) => Self::success(name, step_id, action_id, duration_ms),
            Err(err) => Self::failure(
                name,
                step_id,
                action_id,
                duration_ms,
                err.kind(),
                err.is_permission_denied(),
                err.action_source(),
            ),
        }
    }

    /// Build from a typed [`ActionError`] (IPC invoke / UI probe).
    pub fn from_action(
        name: impl Into<String>,
        step_id: impl Into<String>,
        action_id: impl Into<String>,
        duration_ms: u64,
        result: Result<(), &ActionError>,
    ) -> Self {
        match result {
            Ok(()) => Self::success(name, step_id, action_id, duration_ms),
            Err(err) => Self::failure(
                name,
                step_id,
                action_id,
                duration_ms,
                err.kind(),
                err.is_permission_denied(),
                Some(err),
            ),
        }
    }

    /// Whether this entry records a permission / policy denial.
    pub fn is_denied(&self) -> bool {
        self.denied
    }

    fn success(
        name: impl Into<String>,
        step_id: impl Into<String>,
        action_id: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        let action_id = action_id.into();
        Self {
            name: name.into(),
            step_id: step_id.into(),
            action_id: action_id.clone(),
            duration_ms,
            ok: true,
            error_kind: None,
            denied: false,
            ui_phase: infer_ui_phase(&action_id),
            error_code: None,
            selector_hint: None,
        }
    }

    fn failure(
        name: impl Into<String>,
        step_id: impl Into<String>,
        action_id: impl Into<String>,
        duration_ms: u64,
        kind: String,
        denied: bool,
        action: Option<&ActionError>,
    ) -> Self {
        let action_id = action_id.into();
        Self {
            name: name.into(),
            step_id: step_id.into(),
            action_id: action_id.clone(),
            duration_ms,
            ok: false,
            error_kind: Some(kind),
            denied,
            ui_phase: infer_ui_phase(&action_id),
            error_code: action.and_then(|a| a.ui_code()).map(str::to_string),
            selector_hint: action.and_then(|a| a.selector_hint()).map(str::to_string),
        }
    }
}

fn default_denied() -> bool {
    false
}

fn infer_ui_phase(action_id: &str) -> Option<String> {
    if !action_id.starts_with("ui.") && action_id != "shell.run" {
        return None;
    }
    Some(
        if action_id == "shell.run" {
            "launch"
        } else if action_id.contains("element") && action_id.contains("wait") {
            "login"
        } else if action_id.contains("element") {
            "act"
        } else if action_id.starts_with("ui.window") {
            "verify"
        } else {
            "act"
        }
        .into(),
    )
}

/// Actions whose params must not appear in logs.
pub fn is_sensitive_action(action_id: &str) -> bool {
    matches!(
        action_id,
        "http.send" | "capture.ocr" | "clipboard.get" | "clipboard.set"
    ) || action_id.starts_with("capture.")
}

/// Append-only JSONL audit writer (`audit.jsonl`).
#[derive(Debug, Clone)]
pub struct ExecutionAudit {
    path: PathBuf,
}

impl ExecutionAudit {
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { path })
    }

    pub fn under_data_dir(data_dir: &Path) -> std::io::Result<Self> {
        Self::open(data_dir.join("audit.jsonl"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, entry: &AuditEntry) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        file.write_all(b"\n")?;
        file.flush()?;
        debug!(
            action_id = %entry.action_id,
            step_id = %entry.step_id,
            ok = entry.ok,
            duration_ms = entry.duration_ms,
            "已写入审计记录"
        );
        Ok(())
    }

    pub fn record_best_effort(&self, entry: &AuditEntry) {
        if let Err(e) = self.append(entry) {
            warn!(path = %self.path.display(), error = %e, "写入审计失败");
        }
    }

    pub fn read_all(&self) -> std::io::Result<Vec<AuditEntry>> {
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
            if let Ok(e) = serde_json::from_str::<AuditEntry>(line) {
                out.push(e);
            }
        }
        Ok(out)
    }
}

/// Emit a redacted step log line (no body / OCR / clipboard payloads).
pub fn log_step_start(name: &str, step_id: &str, action_id: &str) {
    info!(
        directive = %name,
        step = %step_id,
        action = %action_id,
        "执行步骤"
    );
}

pub fn log_step_end(entry: &AuditEntry) {
    info!(
        directive = %entry.name,
        step = %entry.step_id,
        action = %entry.action_id,
        duration = entry.duration_ms,
        ok = entry.ok,
        denied = entry.denied,
        "步骤完成"
    );
}

#[allow(dead_code)]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_sensitive_actions() {
        assert!(is_sensitive_action("http.send"));
        assert!(is_sensitive_action("capture.ocr"));
        assert!(!is_sensitive_action("template.render"));
    }

    #[test]
    fn classify_ui_error_code() {
        let err = ActionError::ui("ui_login_pending", "等待元素消失超时");
        assert_eq!(err.ui_code(), Some("ui_login_pending"));
        assert_eq!(err.kind(), "ui_login_pending");
        assert_eq!(infer_ui_phase("ui.element.wait"), Some("login".into()));
    }

    #[test]
    fn extract_selector_hint_from_ui_error() {
        let msg = "[ui_login_pending][selector_hint=name=进入微信] 等待元素消失超时";
        let err = ActionError::execution(msg);
        assert_eq!(err.ui_code(), Some("ui_login_pending"));
        assert_eq!(err.selector_hint(), Some("name=进入微信"));
        let entry =
            AuditEntry::from_action("wechat", "wait_login", "ui.element.wait", 100, Err(&err));
        assert_eq!(entry.error_code.as_deref(), Some("ui_login_pending"));
        assert_eq!(entry.selector_hint.as_deref(), Some("name=进入微信"));
        assert_eq!(entry.ui_phase.as_deref(), Some("login"));
    }

    #[test]
    fn from_engine_sets_denied_on_permission_denied() {
        let err = EngineError::Action(ActionError::PermissionDenied("shell".into()));
        let entry = AuditEntry::from_engine("demo", "s1", "shell.run", 3, Err(&err));
        assert!(!entry.ok);
        assert!(entry.denied);
        assert!(entry.is_denied());
        assert_eq!(entry.error_kind.as_deref(), Some("permission_denied"));
    }

    #[test]
    fn append_audit_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let audit = ExecutionAudit::under_data_dir(dir.path()).unwrap();
        let e = AuditEntry::from_engine("demo", "s1", "file.write", 5, Ok(()));
        audit.append(&e).unwrap();
        let all = audit.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].action_id, "file.write");
        assert!(all[0].ok);
    }
}
