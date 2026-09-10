//! 检查那些写错却不致命的配置值。
//!
//! 范围刻意收窄：只覆盖“写错一个字母就会悄悄换成另一种行为、
//! 而任何地方都不报错”的值。属于某个子系统的校验就交给它自己——
//! `corex update` 自己会校验 `[update].repository` 并夹紧 `timeout_secs`，
//! 这里再查一遍只会多出一个事实来源。

use crate::context::RuntimeConfig;

/// 一个不可能按字面意思生效的配置值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigIssue {
    /// 点分键，如 `runtime.ui_profile`。
    pub key: &'static str,
    /// 哪里不对，以及实际会怎样。
    pub message: String,
}

/// [`UiProfilePreset::parse`] 认得的预设。
const UI_PROFILES: [&str; 4] = ["fast", "patient", "default", "baseline"];

/// 会被当作“级别”而不是“target 名”的那些词。
const LOG_LEVELS: [&str; 6] = ["trace", "debug", "info", "warn", "error", "off"];

/// 报告那些会悄悄回退成别的行为的取值。
pub fn validate(config: &RuntimeConfig) -> Vec<ConfigIssue> {
    let mut issues = Vec::new();

    let profile = config.ui_profile.trim().to_ascii_lowercase();
    if !profile.is_empty() && !UI_PROFILES.contains(&profile.as_str()) {
        issues.push(ConfigIssue {
            key: "runtime.ui_profile",
            message: format!(
                "`{}` 不是已知预设（{}），已按 baseline 处理",
                config.ui_profile,
                UI_PROFILES.join(" / ")
            ),
        });
    }

    let level = config.logging.level.trim().to_ascii_lowercase();
    if !level.is_empty() && !LOG_LEVELS.contains(&level.as_str()) {
        issues.push(ConfigIssue {
            key: "logging.level",
            // `tracing` 的 EnvFilter 会把不认识的词当成 *target* 过滤器，
            // 所以级别写错不是警告一次，而是把所有日志都丢掉。
            message: format!(
                "`{}` 不是日志级别；tracing 会把它当作 target 过滤，日志将几乎全部丢失（可用 {}）",
                config.logging.level,
                LOG_LEVELS.join(" / ")
            ),
        });
    }

    issues
}
