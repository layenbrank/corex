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
pub use definition::{
    permission_kind_for, validate_permissions, Condition, InputDecl, OnError, PermissionKind,
    Permissions, Directive, Step, Trigger,
};
pub use history::{ExecutionHistory, HistoryEntry};
pub use inputs::{apply_input_defaults, is_input_unset};
pub use pipeline::Pipeline;
pub use resolver::Resolver;
