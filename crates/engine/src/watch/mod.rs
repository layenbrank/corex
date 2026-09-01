//! File watch trigger engine.
//!
//! Timing pipeline: FS debounce (`notify_debouncer_full`) → lodash-like throttle
//! (`throttle_ms` interval, leading+trailing) → pipeline run.

#[cfg(feature = "watch")]
pub mod engine;
#[cfg(feature = "watch")]
pub mod event;
#[cfg(feature = "watch")]
pub mod filter;
#[cfg(feature = "watch")]
pub mod throttle;

#[cfg(feature = "watch")]
pub use engine::{WatchEngine, WatchJobSpec};
