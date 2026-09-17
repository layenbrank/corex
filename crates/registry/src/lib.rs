//! 动作注册表与内置动作。

pub mod builtin;
/// 动作目录：注册表的机器可读形态，CLI / daemon / MCP 共用。
pub mod catalog;

#[cfg(feature = "act-ui")]
pub mod ui_probe;

#[cfg(all(feature = "act-ui", windows))]
pub mod ui_pick;

#[cfg(feature = "wasm")]
pub mod discovery;
#[cfg(feature = "wasm")]
pub mod wasm_host;

use corex_core::{Action, ActionMeta, ActionStore, PluginConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

/// 线程安全的具名动作注册表。
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

    /// 注册全部已启用 feature 的内置动作。
    pub fn register_builtins(&mut self) {
        builtin::register_all(self);
    }

    pub fn register(&mut self, action: Arc<dyn Action>) {
        let id = action.meta().id.clone();
        // 用 `debug` 而不是 `info`：这条每个内置动作启动时都会发一次，
        // 用 `info` 会让每条命令都把整个注册表打到 stderr。
        debug!(action = %id, "注册动作");
        self.actions.insert(id, action);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Action>> {
        self.actions.get(id).cloned()
    }

    pub fn actions(&self) -> Vec<ActionMeta> {
        let mut actions: Vec<_> = self.actions.values().map(|a| a.meta()).collect();
        actions.sort_by(|a, b| a.id.cmp(&b.id));
        actions
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

    /// 按配置剔除被禁用的插件与动作。
    pub fn remove_disabled(&mut self, plugins: &PluginConfig) {
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
    fn find_action(&self, id: &str) -> Option<Arc<dyn Action>> {
        self.get(id)
    }

    fn actions(&self) -> Vec<ActionMeta> {
        ActionRegistry::actions(self)
    }
}
