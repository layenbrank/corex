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
pub use corex_core::{PermissionKind, permission_kind_for};
pub use definition::{
    Condition, Directive, InputDecl, OnError, Permissions, Step, Trigger, validate_permissions,
};
pub use history::{ExecutionHistory, HistoryEntry};
pub use inputs::{apply_input_defaults, is_input_unset};
pub use pipeline::Pipeline;
pub use resolver::Resolver;
pub use run::{DirectiveRunner, run_directive_file};
pub use supervisor::process::{
    child_supervisor_identity, current_supervisor_identity, is_pid_running, is_supervisor_alive,
    kill_process_tree, spawn_detached,
};
#[cfg(feature = "cron")]
pub use supervisor::supervise_cron_job;
#[cfg(feature = "watch")]
pub use supervisor::supervise_watch_job;
pub use supervisor::{ControlMsg, JobKind, JobMeta, poll_control, send_control};
pub use trigger::{
    CronConfig, DEBOUNCE_MS, THROTTLE_MS, WatchConfig, find_cron_trigger, find_watch_trigger,
};

#[cfg(feature = "cron")]
pub use cron::{
    CronEngine, CronJobSpec, ResolvedCronTz, bind_cron_engine, effective_cron_timezone,
    find_cron_engine, parse_cron_expr, parse_cron_timezone,
};
#[cfg(feature = "watch")]
pub use watch::filter::{WatchFilter, path_matches, watch_relative_path};
#[cfg(feature = "watch")]
pub use watch::{WatchEngine, WatchJobSpec};
