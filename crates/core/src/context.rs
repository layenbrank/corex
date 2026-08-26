//! Execution context passed into every action invocation.

use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Baseline UI profile name (`[runtime].ui_profile`).
pub const UI_PROFILE: &str = "baseline";

/// Max parallel steps when `[runtime].max_parallel` is omitted.
pub const MAX_PARALLEL: usize = 8;

/// Baseline `selectors[]` chain cap for `ui.element.*`.
pub const MAX_SELECTOR_CHAIN: usize = 8;

/// Runtime knobs loaded from `config/corex.toml` (and overrides).
pub const RUNTIME_CONFIG: &str = "config/corex.toml";

/// Runtime plugin / action enablement from config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Plugin directory (absolute or relative to data dir).
    #[serde(default = "init_plugin_dir")]
    pub plugin_dir: PathBuf,
    /// Disable entire plugins by id.
    #[serde(default)]
    pub disabled: Vec<String>,
    /// Disable individual action ids (e.g. `shell.run`).
    #[serde(default)]
    pub disabled_actions: Vec<String>,
}

fn init_plugin_dir() -> PathBuf {
    PathBuf::from("plugins")
}

/// Append-only execution history settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    /// When true, pipeline executions are recorded to JSONL.
    #[serde(default = "init_history_enabled")]
    pub enabled: bool,
    /// File name or path relative to the data directory.
    #[serde(default = "init_history_file")]
    pub file: PathBuf,
}

fn init_history_enabled() -> bool {
    true
}

fn init_history_file() -> PathBuf {
    PathBuf::from("history.jsonl")
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: init_history_enabled(),
            file: init_history_file(),
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
    #[serde(default = "init_log_level")]
    pub level: String,
    #[serde(default)]
    pub json: bool,
}

fn init_log_level() -> String {
    "info".into()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: init_log_level(),
            json: false,
        }
    }
}

/// UI automation presets from `[runtime].ui_profile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiProfilePreset {
    pub max_selector_chain: usize,
    pub max_settle_ms: u64,
}

impl UiProfilePreset {
    pub fn parse(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "fast" => Self {
                max_selector_chain: 5,
                max_settle_ms: 2_000,
            },
            "patient" => Self {
                max_selector_chain: 12,
                max_settle_ms: 0,
            },
            // legacy alias
            "default" | "baseline" | "" => Self::baseline(),
            _ => Self::baseline(),
        }
    }

    pub fn baseline() -> Self {
        Self {
            max_selector_chain: MAX_SELECTOR_CHAIN,
            max_settle_ms: 0,
        }
    }
}

/// Runtime knobs loaded from `config/corex.toml` (and overrides).
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
    #[serde(default = "init_max_parallel")]
    pub max_parallel: usize,
    #[serde(default)]
    pub step_timeout_secs: u64,
    /// When true, directives with no declared permissions are denied (enterprise mode).
    #[serde(default)]
    pub strict_permissions: bool,
    /// Allowed filesystem roots for file.* actions. Empty = no confine (dev mode).
    #[serde(default)]
    pub filesystem_roots: Vec<PathBuf>,
    /// UI preset: `baseline` | `fast` | `patient` (see [`UiProfilePreset`]).
    #[serde(default = "init_ui_profile")]
    pub ui_profile: String,
    /// Max `selectors[]` fallback chain length for `ui.element.*` (0 = use profile preset).
    #[serde(default)]
    pub ui_max_selector_chain: usize,
    /// Cap total fixed `ui.wait` ms per directive run (0 = unlimited).
    #[serde(default)]
    pub ui_max_settle_ms: u64,
}

fn init_ui_profile() -> String {
    UI_PROFILE.into()
}

fn init_max_parallel() -> usize {
    MAX_PARALLEL
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let preset = UiProfilePreset::baseline();
        Self {
            plugins: PluginConfig::default(),
            history: HistoryConfig::default(),
            daemon: DaemonConfig::default(),
            logging: LoggingConfig::default(),
            max_parallel: init_max_parallel(),
            step_timeout_secs: 0,
            strict_permissions: false,
            filesystem_roots: Vec::new(),
            ui_profile: init_ui_profile(),
            ui_max_selector_chain: preset.max_selector_chain,
            ui_max_settle_ms: preset.max_settle_ms,
        }
    }
}

impl RuntimeConfig {
    /// Resolve selector chain limit (explicit `ui_max_selector_chain` wins over profile).
    pub fn effective_ui_max_selector_chain(&self) -> usize {
        if self.ui_max_selector_chain > 0 {
            return self.ui_max_selector_chain;
        }
        UiProfilePreset::parse(&self.ui_profile).max_selector_chain
    }

    /// Resolved settle cap (explicit `ui_max_settle_ms` when set in config, else profile).
    pub fn effective_ui_max_settle_ms(&self) -> u64 {
        self.ui_max_settle_ms
    }

    /// Apply `ui_profile` preset; explicit overrides in `overrides` win.
    pub fn apply_ui_profile(
        &mut self,
        profile: &str,
        overrides: UiProfileOverrides,
    ) {
        self.ui_profile = profile.to_string();
        let preset = UiProfilePreset::parse(profile);
        self.ui_max_selector_chain = overrides
            .max_selector_chain
            .unwrap_or(preset.max_selector_chain);
        self.ui_max_settle_ms = overrides.max_settle_ms.unwrap_or(preset.max_settle_ms);
    }
}

/// Optional explicit UI runtime overrides from config file.
#[derive(Debug, Clone, Copy, Default)]
pub struct UiProfileOverrides {
    pub max_selector_chain: Option<usize>,
    pub max_settle_ms: Option<u64>,
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
        let max = self.config.effective_ui_max_settle_ms();
        let next = self.ui_session.settle_ms_used.saturating_add(ms);
        if max > 0 && next > max {
            return Err(format!(
                "ui.wait 累计 {next}ms 超过 ui_max_settle_ms={max}"
            ));
        }
        self.ui_session.settle_ms_used = next;
        Ok(())
    }

    /// Max `selectors[]` length for `ui.element.*` (from runtime config / profile).
    pub fn ui_max_selector_chain(&self) -> usize {
        self.config.effective_ui_max_selector_chain()
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

#[cfg(test)]
mod ui_profile_tests {
    use super::*;

    #[test]
    fn baseline_profile_selector_chain_is_8() {
        let cfg = RuntimeConfig::default();
        assert_eq!(cfg.ui_profile, UI_PROFILE);
        assert_eq!(cfg.effective_ui_max_selector_chain(), MAX_SELECTOR_CHAIN);
    }

    #[test]
    fn legacy_default_profile_alias() {
        assert_eq!(
            UiProfilePreset::parse("default").max_selector_chain,
            MAX_SELECTOR_CHAIN
        );
    }

    #[test]
    fn patient_profile_via_apply() {
        let mut cfg = RuntimeConfig::default();
        cfg.apply_ui_profile("patient", UiProfileOverrides::default());
        assert_eq!(cfg.effective_ui_max_selector_chain(), 12);
    }

    #[test]
    fn explicit_chain_overrides_profile() {
        let mut cfg = RuntimeConfig::default();
        cfg.apply_ui_profile("fast", UiProfileOverrides {
            max_selector_chain: Some(10),
            max_settle_ms: None,
        });
        assert_eq!(cfg.effective_ui_max_selector_chain(), 10);
    }
}
