//! Action registry and built-in actions.

pub mod builtin;

#[cfg(feature = "act-ui")]
pub mod ui_probe;

#[cfg(all(feature = "act-ui", windows))]
pub mod ui_pick;

#[cfg(feature = "wasm")]
pub mod discovery;
#[cfg(feature = "wasm")]
pub mod wasm_host;

use corex_core::{Action, ActionMeta, ActionStore, PluginConfig, RuntimeConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// Thread-safe registry of named actions.
#[derive(Default, Clone)]
pub struct ActionRegistry {
    actions: HashMap<String, Arc<dyn Action>>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    /// Register all feature-enabled builtins.
    pub fn register_builtins(&mut self) {
        builtin::register_all(self);
    }

    pub fn register(&mut self, action: Arc<dyn Action>) {
        let id = action.meta().id.clone();
        info!(action = %id, "注册动作");
        self.actions.insert(id, action);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Action>> {
        self.actions.get(id).cloned()
    }

    pub fn list(&self) -> Vec<ActionMeta> {
        let mut list: Vec<_> = self.actions.values().map(|a| a.meta()).collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    pub fn contains(&self, id: &str) -> bool {
        self.actions.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Apply runtime plugin / action disablement from config.
    pub fn apply_runtime_config(&mut self, config: &RuntimeConfig) {
        self.apply_plugin_config(&config.plugins);
    }

    pub fn apply_plugin_config(&mut self, plugins: &PluginConfig) {
        if !plugins.disabled.is_empty() {
            let before = self.actions.len();
            self.actions.retain(|id, _| {
                let plugin = id.split('.').next().unwrap_or(id);
                !plugins.disabled.iter().any(|d| d == plugin || d == id)
            });
            let removed = before - self.actions.len();
            if removed > 0 {
                warn!(removed, "已按 plugins.disabled 移除动作");
            }
        }
        for id in &plugins.disabled_actions {
            if self.actions.remove(id).is_some() {
                warn!(action = %id, "已按 plugins.disabled_actions 禁用动作");
            }
        }
    }

    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

impl ActionStore for ActionRegistry {
    fn get_action(&self, id: &str) -> Option<Arc<dyn Action>> {
        self.get(id)
    }

    fn list_actions(&self) -> Vec<ActionMeta> {
        self.list()
    }
}
