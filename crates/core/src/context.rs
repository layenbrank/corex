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

/// Append-only execution history settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    /// When true, pipeline executions are recorded to JSONL.
    #[serde(default = "default_history_enabled")]
    pub enabled: bool,
    /// File name or path relative to the data directory.
    #[serde(default = "default_history_file")]
    pub file: PathBuf,
}

fn default_history_enabled() -> bool {
    true
}

fn default_history_file() -> PathBuf {
    PathBuf::from("history.jsonl")
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: default_history_enabled(),
            file: default_history_file(),
        }
    }
}

/// Daemon IPC / lock settings from `[daemon]` in config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Unix socket path (relative to data dir) or Windows named pipe path.
    #[serde(default)]
    pub socket_path: Option<PathBuf>,
    /// Singleton lock file (relative to data dir when not absolute).
    #[serde(default)]
    pub lock_path: Option<PathBuf>,
    /// Shared secret for IPC. Empty / unset → auto-generate into data-dir `token`.
    #[serde(default)]
    pub token: Option<String>,
}

/// Logging settings from `[logging]` in config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub json: bool,
}

fn default_log_level() -> String {
    "info".into()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            json: false,
        }
    }
}

/// Runtime knobs loaded from `config/default.toml` (and overrides).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub plugins: PluginConfig,
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default = "default_max_parallel")]
    pub max_parallel: usize,
    #[serde(default)]
    pub step_timeout_secs: u64,
    /// When true, directives with no declared permissions are denied (enterprise mode).
    #[serde(default)]
    pub strict_permissions: bool,
    /// Allowed filesystem roots for file.* actions. Empty = no confine (dev default).
    #[serde(default)]
    pub filesystem_roots: Vec<PathBuf>,
    /// Cap total fixed `ui.wait` ms per directive run (0 = unlimited).
    #[serde(default)]
    pub ui_max_settle_ms: u64,
}

fn default_max_parallel() -> usize {
    8
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            plugins: PluginConfig::default(),
            history: HistoryConfig::default(),
            daemon: DaemonConfig::default(),
            logging: LoggingConfig::default(),
            max_parallel: default_max_parallel(),
            step_timeout_secs: 0,
            strict_permissions: false,
            filesystem_roots: Vec::new(),
            ui_max_settle_ms: 0,
        }
    }
}

/// Cached UI automation scope for a directive run.
#[derive(Debug, Clone, Default)]
pub struct UiSession {
    pub scope_hwnd: Option<i64>,
    pub scope_title: Option<String>,
    /// Accumulated fixed sleep from `ui.wait` (ms).
    pub settle_ms_used: u64,
}

/// Mutable state available while a Directive / pipeline runs.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// User-defined / Directive variables.
    pub variables: HashMap<String, Value>,
    /// Declared Directive inputs resolved at run time.
    pub input: HashMap<String, Value>,
    /// Optional payload from a launcher / previous directive.
    pub directive_input: Option<Value>,
    /// Outputs of completed steps keyed by step id.
    pub step_outputs: HashMap<String, Value>,
    /// Process environment snapshot (string values).
    pub env: HashMap<String, String>,
    /// Runtime configuration.
    pub config: RuntimeConfig,
    /// UI automation session (window scope, settle budget).
    pub ui_session: UiSession,
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
            directive_input: None,
            step_outputs: HashMap::new(),
            env,
            config,
            ui_session: UiSession::default(),
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

    pub fn with_directive_input(mut self, value: Value) -> Self {
        self.directive_input = Some(value);
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

    pub fn set_ui_scope(&mut self, hwnd: i64, title: Option<String>) {
        self.ui_session.scope_hwnd = Some(hwnd);
        self.ui_session.scope_title = title;
    }

    pub fn add_ui_settle_ms(&mut self, ms: u64) -> Result<(), String> {
        let max = self.config.ui_max_settle_ms;
        let next = self.ui_session.settle_ms_used.saturating_add(ms);
        if max > 0 && next > max {
            return Err(format!(
                "ui.wait 累计 {next}ms 超过 ui_max_settle_ms={max}"
            ));
        }
        self.ui_session.settle_ms_used = next;
        Ok(())
    }

    /// Merge outputs (and newly written variables) from a parallel branch context.
    ///
    /// Branch `step_outputs` always win for colliding keys. Variables present in
    /// `other` that differ from `self` are copied over (last-writer-wins by call order).
    pub fn merge_from_branch(&mut self, other: &ExecutionContext) {
        for (k, v) in &other.step_outputs {
            self.step_outputs.insert(k.clone(), v.clone());
        }
        for (k, v) in &other.variables {
            match self.variables.get(k) {
                Some(existing) if existing == v => {}
                _ => {
                    self.variables.insert(k.clone(), v.clone());
                }
            }
        }
    }
}
