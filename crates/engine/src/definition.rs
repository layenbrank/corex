//! Directive YAML definitions.

use corex_core::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level Directive document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Directive {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub inputs: Vec<InputDecl>,
    #[serde(default)]
    pub variables: HashMap<String, Value>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub permissions: Permissions,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub on_error: OnError,
}

impl Directive {
    pub fn from_yaml_str(s: &str) -> Result<Self, corex_core::EngineError> {
        serde_yml::from_str(s).map_err(|e| {
            corex_core::EngineError::ParseError(format!("YAML 解析失败: {e}"))
        })
    }

    pub fn from_yaml_file(path: &Path) -> Result<Self, corex_core::EngineError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_yaml_str(&text)
    }
}

/// Declared Directive input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputDecl {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
}

/// A single pipeline step or control-flow node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Step {
    /// Regular action invocation.
    Action(ActionStep),
    /// Conditional branch.
    If(IfStep),
    /// Repeat / loop.
    Repeat(RepeatStep),
    /// Run child steps in parallel.
    Parallel(ParallelStep),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStep {
    pub id: String,
    /// Action id, e.g. `shell.run`, `file.write`.
    pub action: String,
    #[serde(default)]
    pub params: Value,
    /// Save step output into a variable name.
    #[serde(default)]
    pub save_to: Option<String>,
    #[serde(default)]
    pub when: Option<Condition>,
    #[serde(default)]
    pub on_error: Option<OnError>,
    #[serde(default)]
    pub retry: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfStep {
    pub id: String,
    #[serde(rename = "if")]
    pub condition: Condition,
    pub then: Vec<Step>,
    #[serde(default, rename = "else")]
    pub else_steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeatStep {
    pub id: String,
    pub repeat: RepeatSpec,
    pub steps: Vec<Step>,
}

/// Loop specification: either `count` or `each` must be set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepeatSpec {
    #[serde(default)]
    pub count: Option<u64>,
    /// Expression resolving to a list, e.g. `"{{items}}"`.
    #[serde(default)]
    pub each: Option<String>,
    #[serde(default = "default_item_var", rename = "as")]
    pub as_var: String,
    #[serde(default = "default_index_var", rename = "index")]
    pub index_var: String,
}

fn default_index_var() -> String {
    "index".into()
}

fn default_item_var() -> String {
    "item".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelStep {
    pub id: String,
    pub parallel: Vec<Step>,
    #[serde(default)]
    pub max_concurrency: Option<usize>,
}

/// Boolean / comparison condition for `when` / `if`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    /// Truthy expression string, e.g. `"{{variables.enabled}}"`.
    Expr(String),
    Eq { eq: [Value; 2] },
    Ne { ne: [Value; 2] },
    Gt { gt: [Value; 2] },
    Lt { lt: [Value; 2] },
    And { and: Vec<Condition> },
    Or { or: Vec<Condition> },
    Not { not: Box<Condition> },
}

/// How to react when a step fails.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    #[default]
    Abort,
    Continue,
    Skip,
}

/// Declared permissions a Directive may need.
///
/// When **all** flags are false (YAML omitted / empty), the Directive is treated as
/// unrestricted (allow-all) for backward compatibility with simple directives.
/// Once any flag is `true`, undeclared categories are denied.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub filesystem: bool,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub clipboard: bool,
    #[serde(default)]
    pub notifications: bool,
    #[serde(default)]
    pub ui: bool,
    #[serde(default)]
    pub capture: bool,
    #[serde(default)]
    pub secret: bool,
}

impl Permissions {
    /// `true` when no category was explicitly enabled → allow all actions.
    pub fn is_unrestricted(&self) -> bool {
        !self.network
            && !self.filesystem
            && !self.shell
            && !self.clipboard
            && !self.notifications
            && !self.ui
            && !self.capture
            && !self.secret
    }

    /// Check whether `action_id` is permitted under this declaration.
    pub fn allows_action(&self, action_id: &str) -> Result<(), corex_core::ActionError> {
        if self.is_unrestricted() {
            return Ok(());
        }
        let need = permission_kind_for(action_id);
        let ok = match need {
            PermissionKind::None => true,
            PermissionKind::Network => self.network,
            PermissionKind::Filesystem => self.filesystem,
            PermissionKind::Shell => self.shell,
            PermissionKind::Clipboard => self.clipboard,
            PermissionKind::Notifications => self.notifications,
            PermissionKind::Ui => self.ui,
            PermissionKind::Capture => self.capture,
            PermissionKind::Secret => self.secret,
        };
        if ok {
            Ok(())
        } else {
            Err(corex_core::ActionError::PermissionDenied(format!(
                "指令未声明权限以执行 {action_id}"
            )))
        }
    }
}

/// Permission category required by an action id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionKind {
    None,
    Network,
    Filesystem,
    Shell,
    Clipboard,
    Notifications,
    Ui,
    Capture,
    Secret,
}

/// Map action id → required permission kind.
pub fn permission_kind_for(action_id: &str) -> PermissionKind {
    match action_id {
        "shell.run" | "exec.run" | "bootstrap.env" | "bootstrap.inspect" | "bootstrap.force" => {
            PermissionKind::Shell
        }
        "http.request" => PermissionKind::Network,
        "clipboard.get" | "clipboard.set" => PermissionKind::Clipboard,
        "notify.send" => PermissionKind::Notifications,
        id if id.starts_with("ui.") => PermissionKind::Ui,
        "capture.screenshot" | "capture.monitors" | "capture.ocr" => PermissionKind::Capture,
        id if id.starts_with("keyring.") => PermissionKind::Secret,
        "codec.json.parse" => PermissionKind::None,
        id if id.starts_with("file.")
            || id.starts_with("copy.")
            || id.starts_with("scrub.")
            || id.starts_with("shade.")
            || id.starts_with("compression.")
            || id.starts_with("morph.")
            || id.starts_with("generate.path")
            || id.starts_with("codec.")
            || id == "capture.crop" =>
        {
            PermissionKind::Filesystem
        }
        _ => PermissionKind::None,
    }
}

/// Validate that declared permissions cover all action steps (enterprise `--strict`).
pub fn validate_permissions(directive: &Directive) -> Result<(), String> {
    if directive.permissions.is_unrestricted() {
        return Err(
            "strict: 必须声明 permissions（当前为 unrestricted / allow-all）".into(),
        );
    }
    fn walk(steps: &[Step], perms: &Permissions, errs: &mut Vec<String>) {
        for step in steps {
            match step {
                Step::Action(a) => {
                    if let Err(e) = perms.allows_action(&a.action) {
                        errs.push(format!("{}: {e}", a.id));
                    }
                }
                Step::If(i) => {
                    walk(&i.then, perms, errs);
                    walk(&i.else_steps, perms, errs);
                }
                Step::Repeat(r) => walk(&r.steps, perms, errs),
                Step::Parallel(p) => walk(&p.parallel, perms, errs),
            }
        }
    }
    let mut errs = Vec::new();
    walk(&directive.steps, &directive.permissions, &mut errs);
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrestricted_allows_everything() {
        let p = Permissions::default();
        assert!(p.is_unrestricted());
        assert!(p.allows_action("shell.run").is_ok());
        assert!(p.allows_action("http.request").is_ok());
        assert!(p.allows_action("template.render").is_ok());
    }

    #[test]
    fn filesystem_only_denies_shell_and_network() {
        let p = Permissions {
            filesystem: true,
            ..Permissions::default()
        };
        assert!(!p.is_unrestricted());
        assert!(p.allows_action("file.write").is_ok());
        assert!(p.allows_action("template.render").is_ok()); // None kind
        assert!(p.allows_action("shell.run").is_err());
        assert!(p.allows_action("http.request").is_err());
    }

    #[test]
    fn shell_true_allows_shell_denies_http() {
        let p = Permissions {
            shell: true,
            ..Permissions::default()
        };
        assert!(p.allows_action("shell.run").is_ok());
        assert!(p.allows_action("http.request").is_err());
    }

    #[test]
    fn validate_permissions_rejects_unrestricted() {
        let yaml = r#"
name: bare
steps:
  - id: t
    action: template.render
    params:
      template: "x"
"#;
        let s = Directive::from_yaml_str(yaml).unwrap();
        assert!(validate_permissions(&s).is_err());
    }
}

/// Directive trigger definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    Manual,
    Cron { expr: String },
    FileWatch {
        path: String,
        #[serde(default)]
        debounce_ms: Option<u64>,
    },
    Hotkey { keys: String },
}
