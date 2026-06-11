use std::collections::HashMap;
use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── CLI 参数 ────────────────────────────────────────────────────────────────

/// `corex pipeline` 子命令参数
#[derive(Debug, Clone, Parser)]
pub struct PipelineArgs {
    /// 配置文件路径（默认查找 ~/.corex/pipelines.yaml）
    #[arg(short, long)]
    pub config: Option<String>,

    /// 指定要执行的 Pipeline ID（不指定则交互选择）
    #[arg(short, long)]
    pub id: Option<String>,

    /// 仅验证配置，不执行
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub validate: bool,

    /// Dry-run 模式：只打印步骤信息，不实际执行
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub dry_run: bool,
}

/// `corex schedule` 子命令参数
#[derive(Debug, Clone, Parser)]
pub enum ScheduleArgs {
    /// 交互式选择并执行 Pipeline
    Run,
    /// 生成配置文件模板
    Generate,
    /// 以守护进程模式运行（按 cron 表达式定时执行）
    Cron {
        /// 配置文件路径
        #[arg(short, long)]
        config: Option<String>,
    },
}

// ─── YAML 配置结构 ────────────────────────────────────────────────────────────

/// 顶层配置：`pipelines.yaml`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelinesConfig {
    /// 全局变量（可在所有 pipeline 中引用）
    #[serde(default)]
    pub variables: HashMap<String, String>,

    /// Pipeline 列表
    #[serde(default)]
    pub pipelines: Vec<PipelineConfig>,
}

/// 单条 Pipeline 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    /// Pipeline 唯一标识
    pub id: String,

    /// 描述信息
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    /// cron 表达式（如 `*/5 * * * *`），设置后 schedule cron 模式将定时执行此 Pipeline
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub schedule: Option<String>,

    /// 执行模式：sequential（默认）或 parallel
    #[serde(default)]
    pub mode: ExecutionMode,

    /// 步骤列表
    #[serde(default)]
    pub steps: Vec<StepConfig>,
}

/// 执行模式
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    #[default]
    Sequential,
    Parallel,
}

/// 单个步骤配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepConfig {
    /// 步骤唯一标识（在 pipeline 内唯一）
    pub id: String,

    /// 模块名称：copy / scrub / compression / generate
    pub module: String,

    /// 子动作（可选）：如 generate 的 path / uuid
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub action: Option<String>,

    /// 步骤描述
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    /// 参数（JSON Value，由各 TaskExecutor 自行反序列化）
    #[serde(default)]
    pub params: Value,
}

// ─── 配置加载 ────────────────────────────────────────────────────────────────

/// 查找配置文件，按优先级：pipelines.yaml > pipelines.yml > corex.config.yaml
pub fn find_config_path() -> PathBuf {
    let base = dirs::home_dir().expect("无法获取用户目录").join(".corex");

    let candidates = [
        base.join("pipelines.yaml"),
        base.join("pipelines.yml"),
        base.join("corex.config.yaml"),
        base.join("corex.config.yml"),
    ];

    for path in &candidates {
        if path.exists() {
            return path.clone();
        }
    }

    // 默认返回 pipelines.yaml（即使不存在）
    candidates[0].clone()
}

/// 加载并解析配置文件
pub fn load_config(path: &std::path::Path) -> anyhow::Result<PipelinesConfig> {
    let content =
        std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("读取配置文件失败: {}", e))?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("yaml") | Some("yml") => {
            serde_yml::from_str(&content).map_err(|e| anyhow::anyhow!("解析 YAML 配置失败: {}", e))
        }
        _ => {
            serde_json::from_str(&content).map_err(|e| anyhow::anyhow!("解析 JSON 配置失败: {}", e))
        }
    }
}

/// 校验配置合法性
pub fn validate_config(config: &PipelinesConfig) -> anyhow::Result<()> {
    let ref_re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();

    for pipeline in &config.pipelines {
        // 检查步骤 ID 唯一性
        let mut seen_ids = std::collections::HashSet::new();
        for step in &pipeline.steps {
            if !seen_ids.insert(&step.id) {
                anyhow::bail!("Pipeline '{}' 中步骤 ID '{}' 重复", pipeline.id, step.id);
            }
        }

        // Parallel 模式下禁止跨步骤引用（允许 ${var.xxx}）
        if pipeline.mode == ExecutionMode::Parallel {
            for step in &pipeline.steps {
                if let Value::Object(ref map) = step.params {
                    for (_key, val) in map {
                        if let Value::String(s) = val {
                            for cap in ref_re.captures_iter(s) {
                                let reference = &cap[1];
                                if !reference.starts_with("var.") && reference.contains('.') {
                                    anyhow::bail!(
                                        "Pipeline '{}' 步骤 '{}' 在 parallel 模式下禁止跨步骤引用: ${{{}}}",
                                        pipeline.id,
                                        step.id,
                                        reference
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
