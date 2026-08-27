//! Directive pipeline engine: load YAML, resolve variables, execute steps.

pub mod audit;
pub mod control_flow;
pub mod cron;
pub mod definition;
pub mod history;
pub mod inputs;
pub mod pipeline;
pub mod resolver;
pub mod run;
pub mod supervisor;
pub mod trigger;
pub mod watch;

pub use audit::{AuditEntry, ExecutionAudit};
pub use corex_core::{permission_kind_for, PermissionKind};
pub use definition::{
    validate_permissions, Condition, InputDecl, OnError, Permissions, Directive, Step, Trigger,
};
pub use history::{ExecutionHistory, HistoryEntry};
pub use inputs::{apply_input_defaults, is_input_unset};
pub use pipeline::Pipeline;
pub use resolver::Resolver;
pub use run::{run_directive_file, DirectiveRunner};
pub use supervisor::{poll_control, send_control, ControlMsg, JobKind, JobMeta};
pub use supervisor::process::{is_pid_running, spawn_detached};
#[cfg(feature = "watch")]
pub use supervisor::supervise_watch_job;
#[cfg(feature = "cron")]
pub use supervisor::supervise_cron_job;
pub use trigger::{
    find_cron_trigger, find_watch_trigger, CronConfig, WatchConfig, COOLDOWN_MS,
    DEBOUNCE_MS,
};

#[cfg(feature = "cron")]
pub use cron::{bind_cron_engine, find_cron_engine, parse_cron_expr, CronEngine, CronJobSpec};
#[cfg(feature = "watch")]
pub use watch::{WatchEngine, WatchJobSpec};
#[cfg(feature = "watch")]
pub use watch::filter::path_matches;
