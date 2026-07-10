pub mod schema;
pub mod service;

pub use service::{run, WatchOpts};
pub(crate) use service::{resolve, run_loop, serve, WatchTarget};
