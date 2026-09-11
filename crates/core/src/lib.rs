//! Corex 核心抽象：值、动作、执行上下文与错误。

pub mod action;
pub mod config;
pub mod context;
pub mod error;
pub mod path;
pub mod permission;
pub mod progress;
pub mod schema;
pub mod value;

pub use action::{Action, ActionCategory, ActionMeta, ActionStore, HashMapStore, ParamSchema};
pub use context::{
    DaemonConfig, ExecutionContext, HistoryConfig, LoggingConfig, MAX_PARALLEL, MAX_SELECTOR_CHAIN,
    PluginConfig, RUNTIME_CONFIG, RuntimeConfig, UI_PROFILE, UiProfileOverrides, UiProfilePreset,
    UiSession, UpdateChannel, UpdateConfig, VERSION,
};
pub use error::{ActionError, EngineError};
pub use permission::{PermissionKind, PermissionSet, check_runtime_allowed};
pub use progress::{Mark, Observer, Spot, Unit};
pub use schema::SchemaType;
pub use value::Value;
