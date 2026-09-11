//! 候选路径 → [`RuntimeConfig`]，以及随之产生的告警。

use crate::context::{DaemonConfig, HistoryConfig, LoggingConfig};
use crate::context::{PluginConfig, RuntimeConfig, UiProfileOverrides, UpdateConfig};
use crate::error::EngineError;
use serde::Deserialize;
use std::path::PathBuf;

/// 解析完成的配置、它的来源，以及要告知用户的告警。
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// 文件里的值；没有候选文件时是 [`RuntimeConfig::default`]。
    pub config: RuntimeConfig,
    /// 值的来源文件；`None` 表示正在用默认值。
    pub source: Option<PathBuf>,
    /// 不中断命令、但会静默改变行为的问题。
    pub warnings: Vec<super::ConfigIssue>,
}

/// 读取 `paths` 里第一个存在的文件。
///
/// 文件存在却解析失败是**错误**，不能回退到默认值：`strict_permissions = true`
/// 旁边写错一个字母，不能就这么把门禁悄悄关掉。调用方展示该错误，CLI 里按
/// [`EngineError::Config`] 的状态码退出。
pub fn read(paths: &[PathBuf]) -> Result<ResolvedConfig, EngineError> {
    for path in paths {
        if path.as_os_str().is_empty() || !path.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| EngineError::Config(format!("读取 {} 失败: {e}", path.display())))?;
        let file: ConfigFile = toml::from_str(&text)
            .map_err(|e| EngineError::Config(format!("{} 解析失败: {e}", path.display())))?;

        let config = file.into_runtime();
        let warnings = super::validate(&config);
        return Ok(ResolvedConfig {
            config,
            source: Some(path.clone()),
            warnings,
        });
    }
    Ok(ResolvedConfig {
        config: RuntimeConfig::default(),
        source: None,
        warnings: Vec::new(),
    })
}

/// `corex.toml` 原文。
///
/// 每个章节都可选，所以写一半的文件也是合法的；未知键则会被拒绍——
/// 否则把 `strict_permissions` 写错一个字母也会被接受，
/// 然后被悄悄忽略。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    plugins: Option<PluginConfig>,
    #[serde(default)]
    history: Option<HistoryConfig>,
    #[serde(default)]
    daemon: Option<DaemonConfig>,
    #[serde(default)]
    logging: Option<LoggingConfig>,
    #[serde(default)]
    runtime: Option<RuntimeSection>,
    #[serde(default)]
    update: Option<UpdateConfig>,
}

/// `[runtime]`：直接长在 [`RuntimeConfig`] 上的那些开关。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeSection {
    #[serde(default)]
    max_parallel: Option<usize>,
    #[serde(default)]
    step_timeout: Option<u64>,
    #[serde(default)]
    strict_permissions: Option<bool>,
    #[serde(default)]
    filesystem_roots: Option<Vec<PathBuf>>,
    #[serde(default)]
    ui_profile: Option<String>,
    #[serde(default)]
    ui_selector_depth: Option<usize>,
    #[serde(default)]
    ui_settle_limit: Option<u64>,
    #[serde(default)]
    cron_timezone: Option<String>,
}

impl ConfigFile {
    fn into_runtime(self) -> RuntimeConfig {
        let mut cfg = RuntimeConfig::default();
        if let Some(v) = self.plugins {
            cfg.plugins = v;
        }
        if let Some(v) = self.history {
            cfg.history = v;
        }
        if let Some(v) = self.daemon {
            cfg.daemon = v;
        }
        if let Some(v) = self.logging {
            cfg.logging = v;
        }
        if let Some(v) = self.update {
            cfg.update = v;
        }
        if let Some(r) = self.runtime {
            if let Some(v) = r.max_parallel {
                cfg.max_parallel = v;
            }
            if let Some(v) = r.step_timeout {
                cfg.step_timeout = v;
            }
            if let Some(v) = r.strict_permissions {
                cfg.strict_permissions = v;
            }
            if let Some(v) = r.filesystem_roots {
                cfg.filesystem_roots = v;
            }
            // 具名预设会同时提供两项 UI 上限；没有指名预设时以显式上限为准，
            // `ui_profile` 一直都是这个语义。
            let overrides = UiProfileOverrides {
                selector_depth: r.ui_selector_depth,
                settle_limit: r.ui_settle_limit,
            };
            if let Some(profile) = r.ui_profile {
                cfg.ui_profile(&profile, overrides);
            } else {
                if let Some(depth) = r.ui_selector_depth {
                    cfg.ui_selector_depth = depth;
                }
                if let Some(limit) = r.ui_settle_limit {
                    cfg.ui_settle_limit = limit;
                }
            }
            if let Some(tz) = r.cron_timezone {
                let trimmed = tz.trim();
                if !trimmed.is_empty() {
                    cfg.cron_timezone = trimmed.to_string();
                }
            }
        }
        cfg
    }
}
