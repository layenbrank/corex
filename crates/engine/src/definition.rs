//! Shortcut YAML definitions.

use corex_core::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level shortcut document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shortcut {
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

impl Shortcut {
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

/// Declared shortcut input.
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

/// Declared permissions a shortcut may need.
///
/// When **all** flags are false (YAML omitted / empty), the shortcut is treated as
/// unrestricted (allow-all) for backward compatibility with simple shortcuts.
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
}

impl Permissions {
    /// `true` when no category was explicitly enabled → allow all actions.
    pub fn is_unrestricted(&self) -> bool {
        !self.network
            && !self.filesystem
            && !self.shell
            && !self.clipboard
            && !self.notifications
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
        };
        if ok {
            Ok(())
        } else {
            Err(corex_core::ActionError::PermissionDenied(format!(
                "快捷指令未声明权限以执行 {action_id}"
            )))
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PermissionKind {
    None,
    Network,
    Filesystem,
    Shell,
    Clipboard,
    Notifications,
}

fn permission_kind_for(action_id: &str) -> PermissionKind {
    match action_id {
        "shell.run" | "exec.run" | "bootstrap.env" | "bootstrap.inspect" | "bootstrap.force" => {
            PermissionKind::Shell
        }
        "http.request" | "suggest.bing" => PermissionKind::Network,
        "clipboard.get" | "clipboard.set" | "capture.clipboard" => PermissionKind::Clipboard,
        "notify.send" => PermissionKind::Notifications,
        id if id.starts_with("file.")
            || id.starts_with("copy.")
            || id.starts_with("scrub.")
            || id.starts_with("shade.")
            || id.starts_with("compression.")
            || id.starts_with("morph.")
            || id.starts_with("generate.path")
            || id == "capture.screenshot"
            || id == "capture.crop"
            || id == "capture.monitors" =>
        {
            PermissionKind::Filesystem
        }
        _ => PermissionKind::None,
    }
}

/// Shortcut trigger definitions.
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
