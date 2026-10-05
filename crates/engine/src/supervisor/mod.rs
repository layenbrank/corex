//! PM2 风格的作业 supervisor 工具。

pub mod control;
pub mod job;
pub mod lifecycle;
pub mod process;
pub mod progress;
pub mod resolve;
pub mod run;

pub use control::{ControlMsg, poll_control, send_control};
pub use job::{JobKind, JobMeta};
pub use lifecycle::{JobView, ensure_trigger, jobs, send_job, start_detached, stop_job};
pub use progress::{FileProgress, progress_log_path, run_marker_path};
pub use process::{
    child_supervisor_identity, current_supervisor_identity, is_pid_running, is_supervisor_alive,
    kill_process_tree, spawn_detached,
};
#[cfg(feature = "cron")]
pub use run::supervise_cron_job;
#[cfg(feature = "watch")]
pub use run::supervise_watch_job;
