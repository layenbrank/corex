//! 文件监听触发器引擎。
//!
//! 计时流水线：FS 事件合并（`notify_debouncer_full`，只去重、不计时）→
//! 防抖门（`debounce_ms`）→ 节流门（`throttle_ms`）→ 运行流水线。
//! 两级是同一台 lodash 状态机，差别只有 `maxWait`；`debounce` / `throttle`
//! 分别设置各自执行哪条边沿（`leading` / `trailing`）。

#[cfg(feature = "watch")]
pub mod engine;
#[cfg(feature = "watch")]
pub mod event;
#[cfg(feature = "watch")]
pub mod filter;
#[cfg(feature = "watch")]
pub(crate) mod gate;

#[cfg(feature = "watch")]
pub use engine::{WatchEngine, WatchJobSpec};
