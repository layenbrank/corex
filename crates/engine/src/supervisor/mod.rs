//! PM2-style job supervisor utilities.

pub mod control;
pub mod job;
pub mod process;
pub mod resolve;
pub mod run;

pub use control::{poll_control, send_control, ControlMsg};
pub use job::{JobKind, JobMeta};
pub use process::{
    child_supervisor_identity, current_supervisor_identity, is_pid_running, is_supervisor_alive,
    kill_process_tree, spawn_detached,
};
#[cfg(feature = "cron")]
pub use run::supervise_cron_job;
#[cfg(feature = "watch")]
pub use run::supervise_watch_job;
