//! 文件监听触发器引擎。
//!
//! 计时流水线：FS debounce（`notify_debouncer_full`）→ 类 lodash throttle
//! （`throttle_ms` 间隔，leading+trailing）→ 运行流水线。

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
