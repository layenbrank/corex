//! CLI 的生效配置：读一次，到处用。
//!
//! [`init`] 在 `main` 里、任何命令分派之前运行，这买到两样旧的懒加载做不到的东西：
//!
//! - 文件存在却解析失败会让整次运行失败，而不是把默认值悄悄糊到命令的一部分上；
//! - 文件只读一次，而不是每个消费者读一次（`run::directive` 和 `build_registry` 曾经各读一份）。

use corex_core::RuntimeConfig;
use corex_core::config::ConfigIssue;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// 由 [`init`] 写入；只有测试直接驱动命令时才是 `None`。
static EFFECTIVE: OnceLock<RuntimeConfig> = OnceLock::new();
/// 由 [`init`] 写入：值的来源文件（若有）。
static SOURCE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 读取 `paths` 里第一个存在的文件，并把它设为进程默认配置。
///
/// 返回非致命问题，调用方用各自的输出通道报告。
pub fn init(paths: &[PathBuf]) -> Result<Vec<ConfigIssue>, corex_core::EngineError> {
    let resolved = corex_core::config::read(paths)?;
    let _ = SOURCE.set(resolved.source);
    let _ = EFFECTIVE.set(resolved.config);
    Ok(resolved.warnings)
}

/// 生效配置的来源文件；`None` 表示正在用默认值。
pub fn source() -> Option<&'static Path> {
    SOURCE.get().and_then(|source| source.as_deref())
}

/// 所有命令共用的配置。
///
/// [`init`] 从未被调用时回退到默认值，这样单命令测试不必先装一份配置。
pub fn effective() -> &'static RuntimeConfig {
    EFFECTIVE.get_or_init(RuntimeConfig::default)
}
