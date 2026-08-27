//! Directive pipeline engine: load YAML, resolve variables, execute steps.

pub mod audit;
pub mod control_flow;
pub mod definition;
pub mod history;
pub mod inputs;
pub mod pipeline;
pub mod resolver;
pub mod scheduler;

pub use audit::{AuditEntry, ExecutionAudit};
pub use corex_core::{permission_kind_for, PermissionKind};
pub use definition::{
    validate_permissions, Condition, InputDecl, OnError, Permissions, Directive, Step, Trigger,
};
pub use history::{ExecutionHistory, HistoryEntry};
pub use inputs::{apply_input_defaults, is_input_unset};
pub use pipeline::Pipeline;
pub use resolver::Resolver;
