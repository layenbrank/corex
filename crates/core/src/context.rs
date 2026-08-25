//! Execution context passed into every action invocation.

use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Runtime plugin / action enablement from config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Plugin directory (absolute or relative to data dir).
    #[serde(default = "default_plugin_dir")]
    pub plugin_dir: PathBuf,
    /// Disable entire plugins by id.
    #[serde(default)]
    pub disabled: Vec<String>,
    /// Disable individual action ids (e.g. `shell.run`).
    #[serde(default)]
    pub disabled_actions: Vec<String>,
}

fn default_plugin_dir() -> PathBuf {
    PathBuf::from("plugins")
}

/// Runtime knobs loaded from `config/default.toml` (and overrides).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub plugins: PluginConfig,
    #[serde(default = "default_max_parallel")]
    pub max_parallel: usize,
    #[serde(default)]
    pub step_timeout_secs: u64,
}

fn default_max_parallel() -> usize {
    8
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            plugins: PluginConfig::default(),
            max_parallel: default_max_parallel(),
            step_timeout_secs: 0,
        }
    }
}

/// Mutable state available while a shortcut / pipeline runs.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// User-defined / shortcut variables.
    pub variables: HashMap<String, Value>,
    /// Declared shortcut inputs resolved at run time.
    pub input: HashMap<String, Value>,
    /// Optional payload from a launcher / previous shortcut.
    pub shortcut_input: Option<Value>,
    /// Outputs of completed steps keyed by step id.
    pub step_outputs: HashMap<String, Value>,
    /// Process environment snapshot (string values).
    pub env: HashMap<String, String>,
    /// Runtime configuration.
    pub config: RuntimeConfig,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new(RuntimeConfig::default())
    }
}

impl ExecutionContext {
    pub fn new(config: RuntimeConfig) -> Self {
        let env = std::env::vars().collect();
        Self {
            variables: HashMap::new(),
            input: HashMap::new(),
            shortcut_input: None,
            step_outputs: HashMap::new(),
            env,
            config,
        }
    }

    pub fn with_input(mut self, input: HashMap<String, Value>) -> Self {
        self.input = input;
        self
    }

    pub fn with_variables(mut self, variables: HashMap<String, Value>) -> Self {
        self.variables = variables;
        self
    }

    pub fn with_shortcut_input(mut self, value: Value) -> Self {
        self.shortcut_input = Some(value);
        self
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    pub fn set_step_output(&mut self, step_id: impl Into<String>, value: Value) {
        self.step_outputs.insert(step_id.into(), value);
    }

    pub fn get_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    pub fn get_step_output(&self, step_id: &str) -> Option<&Value> {
        self.step_outputs.get(step_id)
    }
}
