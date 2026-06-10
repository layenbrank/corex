use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::{compression, copy, generate, scrub};

/// schedule 子命令
#[derive(Debug, Parser)]
pub enum Args {
    /// 交互式选择并执行配置任务
    Run,
    /// 在 ~/.corex/ 生成配置文件模板
    Generate,
}

/// ~/.corex/corex.config.yaml 反序列化结构（支持 pipeline 编排）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// 管道列表：每个 pipeline 包含一系列顺序执行的步骤
    #[serde(default)]
    pub pipelines: Vec<Pipeline>,
}

/// 一条管道（流水线）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
    /// 按顺序执行的步骤列表
    #[serde(default)]
    pub steps: Vec<Step>,
}

/// 单个步骤，通过 `type` 字段区分任务类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Step {
    #[serde(rename = "copy")]
    Copy(copy::schema::Args),

    #[serde(rename = "generate-path")]
    GeneratePath(generate::schema::PathArgs),

    #[serde(rename = "generate-uuid")]
    GenerateUuid(generate::schema::UuidArgs),

    #[serde(rename = "compression")]
    Compression(compression::schema::Args),

    #[serde(rename = "scrub")]
    Scrub(scrub::schema::Args),
}
