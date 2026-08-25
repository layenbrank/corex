//! Action trait and metadata.

use crate::context::ExecutionContext;
use crate::error::ActionError;
use crate::schema::SchemaType;
use crate::value::Value;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// High-level grouping for discovery / UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    System,
    Network,
    Data,
    Ui,
    Logic,
    Plugin,
}

/// Declares a single parameter for an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSchema {
    pub name: String,
    pub ty: SchemaType,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

impl ParamSchema {
    pub fn new(name: impl Into<String>, ty: SchemaType, required: bool) -> Self {
        Self {
            name: name.into(),
            ty,
            required,
            description: None,
            default: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_default(mut self, default: impl Into<Value>) -> Self {
        self.default = Some(default.into());
        self
    }
}

/// Static metadata describing an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: ActionCategory,
    #[serde(default)]
    pub params: Vec<ParamSchema>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ActionMeta {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        category: ActionCategory,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            category,
            params: Vec::new(),
            tags: Vec::new(),
        }
    }

    pub fn with_params(mut self, params: Vec<ParamSchema>) -> Self {
        self.params = params;
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// Executable unit registered in the action registry.
#[async_trait]
pub trait Action: Send + Sync {
    fn meta(&self) -> ActionMeta;

    /// Validate parameters before execution. Default: check required params exist.
    async fn validate(&self, params: &Value) -> Result<(), ActionError> {
        let meta = self.meta();
        let map = match params {
            Value::Map(m) => m,
            Value::Null => &BTreeMap::new(),
            _ => {
                return Err(ActionError::InvalidParams(
                    "参数必须是对象（map）".into(),
                ));
            }
        };
        for p in &meta.params {
            if p.required && !map.contains_key(&p.name) {
                return Err(ActionError::MissingParam(p.name.clone()));
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError>;
}

/// Lookup facade so the engine can resolve actions without depending on the registry crate.
pub trait ActionStore: Send + Sync {
    fn get_action(&self, id: &str) -> Option<Arc<dyn Action>>;

    fn list_actions(&self) -> Vec<ActionMeta> {
        Vec::new()
    }
}

impl ActionStore for HashMapStore {
    fn get_action(&self, id: &str) -> Option<Arc<dyn Action>> {
        self.0.get(id).cloned()
    }

    fn list_actions(&self) -> Vec<ActionMeta> {
        self.0.values().map(|a| a.meta()).collect()
    }
}

/// Simple in-memory store used by tests and lightweight runners.
#[derive(Default)]
pub struct HashMapStore(pub std::collections::HashMap<String, Arc<dyn Action>>);

impl HashMapStore {
    pub fn new() -> Self {
        Self(std::collections::HashMap::new())
    }

    pub fn register(&mut self, action: Arc<dyn Action>) {
        let id = action.meta().id;
        self.0.insert(id, action);
    }
}
