//! Step-level audit log (redacted; no bodies / OCR / clipboard content).

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_denied: Option<bool>,
    /// UI automation phase hint (launch/login/act/verify).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_phase: Option<String>,
    /// Structured error code (e.g. ui_login_pending).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Redacted selector hint (no PII); parsed from `[selector_hint=...]` in error text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector_hint: Option<String>,
}

impl AuditEntry {
    pub fn new(
        name: impl Into<String>,
        step_id: impl Into<String>,
        action_id: impl Into<String>,
        duration_ms: u64,
        result: Result<(), String>,
        permission_denied: bool,
    ) -> Self {
        let action_id = action_id.into();
        let (ok, error_kind, error_code, selector_hint) = match &result {
            Ok(()) => (true, None, None, None),
            Err(e) => (
                false,
                Some(classify_error(e)),
                extract_ui_error_code(e),
                extract_selector_hint(e),
            ),
        };
        Self {
            name: name.into(),
            step_id: step_id.into(),
            action_id: action_id.clone(),
            duration_ms,
            ok,
            error_kind,
            permission_denied: if permission_denied { Some(true) } else { None },
            ui_phase: infer_ui_phase(&action_id),
            error_code,
            selector_hint,
        }
    }
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

fn extract_ui_error_code(msg: &str) -> Option<String> {
    for segment in bracket_segments(msg) {
        if segment.starts_with("ui_") {
            return Some(segment.to_string());
        }
    }
    None
}

fn extract_selector_hint(msg: &str) -> Option<String> {
    for segment in bracket_segments(msg) {
        if let Some(hint) = segment.strip_prefix("selector_hint=") {
            if !hint.is_empty() {
                return Some(hint.to_string());
            }
        }
    }
    None
}

fn bracket_segments(msg: &str) -> impl Iterator<Item = &str> {
    msg.split(']').filter_map(|part| {
        let start = part.rfind('[')?;
        Some(&part[start + 1..])
    })
}

fn classify_error(msg: &str) -> String {
    if let Some(code) = extract_ui_error_code(msg) {
        return code;
    }
    let lower = msg.to_lowercase();
    if lower.contains("permission") || msg.contains("权限") {
        "permission_denied".into()
    } else if lower.contains("timeout") || msg.contains("超时") {
        "timeout".into()
    } else {
        "execution".into()
    }
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
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
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
        serde_json::to_writer(&mut file, entry).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e)
        })?;
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
        directive_name = %name,
        step_id = %step_id,
        action_id = %action_id,
        "执行步骤"
    );
}

pub fn log_step_end(entry: &AuditEntry) {
    info!(
        directive_name = %entry.name,
        step_id = %entry.step_id,
        action_id = %entry.action_id,
        duration_ms = entry.duration_ms,
        ok = entry.ok,
        permission_denied = entry.permission_denied.unwrap_or(false),
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
        assert_eq!(
            classify_error("[ui_login_pending] 等待元素消失超时"),
            "ui_login_pending"
        );
        assert_eq!(infer_ui_phase("ui.element.wait"), Some("login".into()));
    }

    #[test]
    fn extract_selector_hint_from_ui_error() {
        let msg = "[ui_login_pending][selector_hint=name=进入微信] 等待元素消失超时";
        assert_eq!(extract_ui_error_code(msg), Some("ui_login_pending".into()));
        assert_eq!(
            extract_selector_hint(msg),
            Some("name=进入微信".into())
        );
        let entry = AuditEntry::new(
            "wechat",
            "wait_login",
            "ui.element.wait",
            100,
            Err(msg.into()),
            false,
        );
        assert_eq!(entry.error_code.as_deref(), Some("ui_login_pending"));
        assert_eq!(entry.selector_hint.as_deref(), Some("name=进入微信"));
        assert_eq!(entry.ui_phase.as_deref(), Some("login"));
    }

    #[test]
    fn append_audit_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let audit = ExecutionAudit::under_data_dir(dir.path()).unwrap();
        let e = AuditEntry::new("demo", "s1", "file.write", 5, Ok(()), false);
        audit.append(&e).unwrap();
        let all = audit.read_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].action_id, "file.write");
        assert!(all[0].ok);
    }
}
