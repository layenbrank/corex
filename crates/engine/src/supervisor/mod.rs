//! PM2-style job supervisor utilities.

pub mod control;
pub mod job;
pub mod process;
pub mod resolve;
pub mod run;

pub use control::{poll_control, send_control, ControlMsg};
pub use job::{JobKind, JobMeta};
pub use process::{is_pid_running, spawn_detached};
#[cfg(any(feature = "cron", feature = "watch"))]
pub use run::{supervise_cron_job, supervise_watch_job};
