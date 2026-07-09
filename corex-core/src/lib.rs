// ─── 核心模块 ────────────────────────────────────────────────────────────────
#[cfg(feature = "command")]
pub mod command;
#[cfg(feature = "pipeline")]
pub mod pipeline;
#[cfg(feature = "schedule")]
pub mod schedule;
#[cfg(feature = "tasks")]
pub mod tasks;
pub mod utils;

// ─── 业务模块（保留内部 schema + service 结构）─────────────────────────────
#[cfg(feature = "bootstrap")]
pub mod bootstrap;
#[cfg(feature = "compression")]
pub mod compression;
#[cfg(feature = "copy")]
pub mod copy;
#[cfg(feature = "generate")]
pub mod generate;
#[cfg(feature = "screenshot")]
pub mod screenshot;
#[cfg(feature = "scrub")]
pub mod scrub;
#[cfg(feature = "shade")]
pub mod shade;

#[cfg(feature = "serve")]
pub mod serve;
