//! Corex core abstractions: values, actions, execution context, and errors.

pub mod action;
pub mod context;
pub mod error;
pub mod path;
pub mod schema;
pub mod value;

pub use action::{Action, ActionCategory, ActionMeta, ActionStore, HashMapStore, ParamSchema};
pub use context::{
    DaemonConfig, ExecutionContext, HistoryConfig, LoggingConfig, PluginConfig, RuntimeConfig,
    UiSession,
};
pub use error::{ActionError, EngineError};
pub use schema::SchemaType;
pub use value::Value;
