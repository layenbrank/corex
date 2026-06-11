use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde_json::Value;

use crate::pipeline::context::PipelineContext;

/// 任务执行输出
#[derive(Debug, Clone, Default)]
pub struct TaskOutput {
    /// 输出路径（供下一步作为输入）
    pub path: Option<PathBuf>,
    /// 任意元数据（键值对形式）
    pub metadata: HashMap<String, Value>,
}

/// 核心任务 trait —— Pipeline 中所有步骤的统一接口
///
/// 每个业务模块（copy、compression、generate 等）都实现此 trait，
/// 以便被 Pipeline Runner 统一调度。
pub trait TaskExecutor: Send + Sync {
    /// 执行任务
    ///
    /// - `params`: 从 YAML 配置反序列化得到的参数（`serde_json::Value`）
    /// - `ctx`: Pipeline 上下文，用于读取/写入步骤间共享数据
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput>;
}

// ─── 各模块的 TaskExecutor 实现 ─────────────────────────────────────────────

/// Copy 任务执行器
pub struct CopyExecutor;

impl TaskExecutor for CopyExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::copy::schema::Args = serde_json::from_value(params.clone())?;

        // 解析变量引用
        let from = ctx.resolve(&args.from);
        let to = ctx.resolve(&args.to);
        let resolved = crate::copy::schema::Args {
            from,
            to: to.clone(),
            empty: args.empty,
            includes: args.includes,
            excludes: args.excludes,
            id: args.id,
            description: args.description,
        };

        crate::copy::service::run(&resolved)?;

        Ok(TaskOutput {
            path: Some(PathBuf::from(&resolved.to)),
            metadata: HashMap::new(),
        })
    }
}

/// Scrub 任务执行器
pub struct ScrubExecutor;

impl TaskExecutor for ScrubExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::scrub::schema::Args = serde_json::from_value(params.clone())?;

        let source = ctx.resolve(&args.source);
        let target = ctx.resolve(&args.target);
        let resolved = crate::scrub::schema::Args {
            source,
            target: target.clone(),
            recursive: args.recursive,
            description: args.description,
        };

        crate::scrub::service::run(&resolved)?;

        Ok(TaskOutput {
            path: Some(PathBuf::from(&resolved.target)),
            metadata: HashMap::new(),
        })
    }
}

/// Compression 任务执行器
pub struct CompressionExecutor;

impl TaskExecutor for CompressionExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::compression::schema::Args = serde_json::from_value(params.clone())?;

        let from = ctx.resolve(&args.from);
        let to = ctx.resolve(&args.to);
        let resolved = crate::compression::schema::Args {
            from,
            to: to.clone(),
            description: args.description,
            id: args.id,
        };

        crate::compression::service::run(&resolved)?;

        Ok(TaskOutput {
            path: Some(PathBuf::from(&resolved.to)),
            metadata: HashMap::new(),
        })
    }
}

/// Generate Path 任务执行器
pub struct GeneratePathExecutor;

impl TaskExecutor for GeneratePathExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::generate::schema::PathArgs = serde_json::from_value(params.clone())?;

        let from = ctx.resolve(&args.from);
        let to = ctx.resolve(&args.to);
        let resolved = crate::generate::schema::PathArgs {
            from,
            to: to.clone(),
            transform: args.transform,
            index: args.index,
            separator: args.separator,
            pad: args.pad,
            includes: args.includes,
            excludes: args.excludes,
            uppercase: args.uppercase,
            id: args.id,
            description: args.description,
        };

        crate::generate::service::path_task(&resolved)?;

        Ok(TaskOutput {
            path: Some(PathBuf::from(&resolved.to)),
            metadata: HashMap::new(),
        })
    }
}

/// Generate UUID 任务执行器
pub struct GenerateUuidExecutor;

impl TaskExecutor for GenerateUuidExecutor {
    fn execute(&self, params: &Value, _ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::generate::schema::UuidArgs = serde_json::from_value(params.clone())?;
        crate::generate::service::uuid_task(&args);
        Ok(TaskOutput::default())
    }
}

// ─── 任务工厂 ────────────────────────────────────────────────────────────────

/// 根据 module + action 创建对应的 TaskExecutor
pub fn create_executor(module: &str, action: Option<&str>) -> Option<Box<dyn TaskExecutor>> {
    match (module, action) {
        ("copy", _) => Some(Box::new(CopyExecutor)),
        ("scrub", _) => Some(Box::new(ScrubExecutor)),
        ("compression", _) => Some(Box::new(CompressionExecutor)),
        ("generate", Some("path")) => Some(Box::new(GeneratePathExecutor)),
        ("generate", Some("uuid")) => Some(Box::new(GenerateUuidExecutor)),
        ("generate", None) => Some(Box::new(GeneratePathExecutor)), // 默认 path
        _ => None,
    }
}
