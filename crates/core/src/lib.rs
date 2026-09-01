//! Corex core abstractions: values, actions, execution context, and errors.

pub mod action;
pub mod context;
pub mod error;
pub mod path;
pub mod permission;
pub mod schema;
pub mod value;

pub use action::{Action, ActionCategory, ActionMeta, ActionStore, HashMapStore, ParamSchema};
pub use context::{
    DaemonConfig, ExecutionContext, HistoryConfig, LoggingConfig, MAX_PARALLEL, MAX_SELECTOR_CHAIN,
    PluginConfig, RUNTIME_CONFIG, RuntimeConfig, UI_PROFILE, UiProfileOverrides, UiProfilePreset,
    UiSession,
};
pub use error::{ActionError, EngineError};
pub use permission::{PermissionKind, check_runtime_allowed, permission_kind_for};
pub use schema::SchemaType;
pub use value::Value;
