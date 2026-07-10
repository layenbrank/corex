// ─── 运行时与集成 ────────────────────────────────────────────────────────────
#[cfg(feature = "invoke")]
pub mod invoke;
#[cfg(feature = "runtime")]
pub mod runtime;

// ─── 核心模块 ────────────────────────────────────────────────────────────────
#[cfg(feature = "command")]
pub mod command;
#[cfg(feature = "pipeline")]
pub mod pipeline;
#[cfg(feature = "schedule")]
pub mod schedule;
#[cfg(feature = "watch")]
pub mod watch;
pub mod utils;

// ─── 业务模块 ────────────────────────────────────────────────────────────────
#[cfg(feature = "bootstrap")]
pub mod bootstrap;
#[cfg(feature = "codec")]
pub mod codec;
#[cfg(feature = "compression")]
pub mod compression;
#[cfg(feature = "copy")]
pub mod copy;
#[cfg(feature = "generate")]
pub mod generate;
#[cfg(feature = "morph")]
pub mod morph;
#[cfg(feature = "scan")]
pub mod scan;
#[cfg(feature = "screenshot")]
pub mod screenshot;
#[cfg(feature = "scrub")]
pub mod scrub;
#[cfg(feature = "shade")]
pub mod shade;

#[cfg(feature = "serve")]
pub mod serve;
