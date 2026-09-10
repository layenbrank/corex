//! 读取并校验 `corex.toml`。
//!
//! CLI、daemon 与 `corex ui` 过去各自带着一份这套逻辑的副本，
//! 而它们漂移了：`corex ui` 只认得六个章节里的两个，于是
//! `[history]`、`[daemon]`、`[logging]`、`[update]` 对它不可见，
//! `runtime.filesystem_roots` 之类也一样。只有一套实现，就只有一个
//! 关于“这台机器的配置是什么”的答案。

mod read;
mod validate;

pub use read::{ResolvedConfig, read};
pub use validate::{ConfigIssue, validate};
