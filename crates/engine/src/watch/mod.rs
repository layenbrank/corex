//! File watch trigger engine.

#[cfg(feature = "watch")]
pub mod engine;
#[cfg(feature = "watch")]
pub mod filter;

#[cfg(feature = "watch")]
pub use engine::{WatchEngine, WatchJobSpec};
