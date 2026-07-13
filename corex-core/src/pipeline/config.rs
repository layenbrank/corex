//! Pipeline v3 配置 schema

use std::collections::HashMap;
use std::path::PathBuf;

use clap::Parser;
use clap::builder::ArgAction;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CONFIG_VERSION: u32 = 3;

/// `corex pipeline` 子命令参数
#[derive(Debug, Clone, Parser)]
pub struct PipelineArgs {
    /// 配置文件路径
    #[arg(short, long)]
    pub config: Option<String>,

    /// 指定 Pipeline ID
    #[arg(short, long)]
    pub id: Option<String>,

    /// 覆盖 variables，如 -D base=D:/proj
    #[arg(short = 'D', value_parser = crate::runtime::parse_define, action = ArgAction::Append)]
    pub define: Vec<(String, String)>,

    /// 仅验证配置
    #[arg(long, action = ArgAction::SetTrue)]
    pub validate: bool,

    /// Dry-run
    #[arg(long, action = ArgAction::SetTrue)]
    pub dry_run: bool,

    /// 单次执行，忽略 yaml 中的 watch/schedule
    #[arg(long, action = ArgAction::SetTrue)]
    pub once: bool,

    /// 写入 JSON 执行报告
    #[arg(long)]
    pub report_file: Option<PathBuf>,
}

/// 顶层 pipelines.yaml（v3）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelinesConfig {
    /// 必须为 3
    pub version: u32,
    #[serde(default)]
    pub variables: HashMap<String, String>,
    #[serde(default)]
    pub pipelines: Vec<PipelineConfig>,
}

/// 单条 Pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub watch: Option<WatchConfig>,
    #[serde(default)]
    pub steps: Vec<StepConfig>,
}

/// 文件监听配置（`corex watch run`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// 监听路径（文件或目录，可多个）
    pub paths: Vec<String>,
    /// glob 白名单（非空时仅匹配这些模式的变更才触发）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<String>,
    /// glob 黑名单（匹配则忽略变更）
    #[serde(default = "default_watch_excludes")]
    pub excludes: Vec<String>,
    /// debounce 毫秒，默认 300
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    /// 执行完成后的冷却毫秒；未设置时取 `max(debounce_ms * 2, 1000)`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_ms: Option<u64>,
}

fn default_watch_excludes() -> Vec<String> {
    vec!["**/.git/**".into(), "**/node_modules/**".into()]
}

fn default_debounce_ms() -> u64 {
    300
}

/// 步骤配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StepConfig {
    pub id: String,
    pub module: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retry: Option<RetryConfig>,
    #[serde(default)]
    pub params: Value,
}

/// 重试策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_max")]
    pub max: u32,
    #[serde(default = "default_backoff")]
    pub backoff_ms: u64,
}

fn default_retry_max() -> u32 {
    3
}

fn default_backoff() -> u64 {
    1000
}

pub fn find_config_path() -> PathBuf {
    let base = dirs::home_dir().expect("无法获取用户目录").join(".corex");
    for name in ["pipelines.yaml", "pipelines.yml", "corex.config.yaml"] {
        let p = base.join(name);
        if p.exists() {
            return p;
        }
    }
    base.join("pipelines.yaml")
}

pub fn load_config(path: &std::path::Path) -> anyhow::Result<PipelinesConfig> {
    let content =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("读取配置文件失败: {}", e))?;
    let config: PipelinesConfig =
        match path.extension().and_then(|e| e.to_str()) {
            Some("yaml") | Some("yml") => serde_yml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("解析 YAML 失败: {}", e))?,
            _ => serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("解析 JSON 失败: {}", e))?,
        };
    if config.version != CONFIG_VERSION {
        anyhow::bail!(
            "配置 version 必须为 {}，当前为 {}",
            CONFIG_VERSION,
            config.version
        );
    }
    Ok(config)
}

pub fn validate_config(config: &PipelinesConfig) -> anyhow::Result<()> {
    use crate::pipeline::graph::StageGraph;

    for pipeline in &config.pipelines {
        let mut seen = std::collections::HashSet::new();
        for step in &pipeline.steps {
            if !seen.insert(&step.id) {
                anyhow::bail!("Pipeline '{}' 步骤 ID '{}' 重复", pipeline.id, step.id);
            }
            if !crate::invoke::known_modules().contains(&step.module.as_str()) {
                anyhow::bail!(
                    "Pipeline '{}' 步骤 '{}' 未知 module: {}",
                    pipeline.id,
                    step.id,
                    step.module
                );
            }
        }
        StageGraph::from_pipeline(pipeline)?.validate()?;
        if let Some(watch) = &pipeline.watch {
            if watch.paths.is_empty() {
                anyhow::bail!(
                    "Pipeline '{}' watch.paths 不能为空",
                    pipeline.id
                );
            }
        }
    }
    Ok(())
}

/// validate 结构化结果（JSON 输出）
#[derive(Debug, Serialize)]
pub struct ValidateReport {
    pub ok: bool,
    pub pipeline_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

impl ValidateReport {
    pub fn success(count: usize) -> Self {
        Self {
            ok: true,
            pipeline_count: count,
            errors: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_config_deserializes_defaults() {
        let yaml = r#"
version: 3
pipelines:
  - id: dev
    watch:
      paths: ["./src"]
    steps:
      - id: scan_os
        module: scan
        params:
          Os: {}
"#;
        let config: PipelinesConfig = serde_yml::from_str(yaml).unwrap();
        let watch = config.pipelines[0].watch.as_ref().unwrap();
        assert_eq!(watch.paths, vec!["./src"]);
        assert!(watch.includes.is_empty());
        assert_eq!(watch.excludes, default_watch_excludes());
        assert_eq!(watch.debounce_ms, 300);
    }

    #[test]
    fn validate_rejects_empty_watch_paths() {
        let config = PipelinesConfig {
            version: CONFIG_VERSION,
            variables: HashMap::new(),
            pipelines: vec![PipelineConfig {
                id: "bad".into(),
                description: None,
                schedule: None,
                watch: Some(WatchConfig {
                    paths: vec![],
                    includes: vec![],
                    excludes: vec![],
                    debounce_ms: 300,
                    cooldown_ms: None,
                }),
                steps: vec![StepConfig {
                    id: "s".into(),
                    module: "scan".into(),
                    description: None,
                    depends_on: vec![],
                    when: None,
                    retry: None,
                    params: serde_json::json!({"Os": {}}),
                }],
            }],
        };
        let err = validate_config(&config).unwrap_err();
        assert!(err.to_string().contains("watch.paths"));
    }
}
