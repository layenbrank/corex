// ─── 核心模块 ────────────────────────────────────────────────────────────────
pub mod cli;
pub mod pipeline;
pub mod tasks;
pub mod utils;

// ─── 业务模块（保留内部 schema + service 结构）─────────────────────────────
pub mod bootstrap;
pub mod compression;
pub mod copy;
pub mod generate;
pub mod screenshot;
pub mod scrub;
