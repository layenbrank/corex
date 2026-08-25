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
    #[serde(default = "default_item_var")]
    pub as_var: String,
    #[serde(default = "default_index_var")]
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
