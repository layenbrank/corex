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

/// Compression ZIP 任务执行器
pub struct CompressionZipExecutor;

impl TaskExecutor for CompressionZipExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::compression::schema::ZipArgs = serde_json::from_value(params.clone())?;

        let from = ctx.resolve(&args.from);
        let to = ctx.resolve(&args.to);
        let resolved = crate::compression::schema::ZipArgs {
            from,
            to: to.clone(),
            description: args.description,
            id: args.id,
        };

        crate::compression::service::run(&crate::compression::schema::Args::Zip(resolved.clone()))?;

        Ok(TaskOutput {
            path: Some(PathBuf::from(&resolved.to)),
            metadata: HashMap::new(),
        })
    }
}

/// Decompression（解压）任务执行器
pub struct DecompressionExecutor;

impl TaskExecutor for DecompressionExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::compression::schema::UnzipArgs = serde_json::from_value(params.clone())?;

        let from = ctx.resolve(&args.from);
        let to = ctx.resolve(&args.to);
        let resolved = crate::compression::schema::UnzipArgs {
            from,
            to: to.clone(),
            description: args.description,
            id: args.id,
        };

        crate::compression::service::run(&crate::compression::schema::Args::Unzip(
            resolved.clone(),
        ))?;

        Ok(TaskOutput {
            path: Some(PathBuf::from(&resolved.to)),
            metadata: HashMap::new(),
        })
    }
}

/// Shade（图片处理）任务执行器
pub struct ShadeExecutor;

impl TaskExecutor for ShadeExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::shade::schema::Args = serde_json::from_value(params.clone())?;

        let from = ctx.resolve(&args.from);
        let to = ctx.resolve(&args.to);
        let resolved = crate::shade::schema::Args {
            from,
            to: to.clone(),
            format: args.format,
            quality: args.quality,
            description: args.description,
            id: args.id,
        };

        crate::shade::service::run(&resolved)?;

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

/// Generate File 任务执行器
pub struct GenerateFileExecutor;

impl TaskExecutor for GenerateFileExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::generate::schema::FileArgs = serde_json::from_value(params.clone())?;

        let to = ctx.resolve(&args.to);
        let template = args.template.as_ref().map(|s| ctx.resolve(s));
        let fragment = args.fragment.as_ref().map(|s| ctx.resolve(s));
        let variable = args
            .variable
            .iter()
            .map(|(k, v)| (k.clone(), ctx.resolve(v)))
            .collect();

        let resolved = crate::generate::schema::FileArgs {
            to: to.clone(),
            template,
            fragment,
            variable,
            id: args.id,
            description: args.description,
        };

        crate::generate::service::file_task(&resolved)?;

        Ok(TaskOutput {
            path: Some(PathBuf::from(&resolved.to)),
            metadata: HashMap::new(),
        })
    }
}

/// Screenshot 任务执行器
pub struct ScreenshotExecutor;

impl TaskExecutor for ScreenshotExecutor {
    fn execute(&self, params: &Value, ctx: &mut PipelineContext) -> Result<TaskOutput> {
        let args: crate::screenshot::schema::Args = serde_json::from_value(params.clone())?;

        let to = ctx.resolve(&args.to);
        let resolved = crate::screenshot::schema::Args {
            to: to.clone(),
            description: args.description,
        };

        crate::screenshot::service::run(&resolved)?;

        Ok(TaskOutput {
            path: Some(PathBuf::from(&resolved.to)),
            metadata: HashMap::new(),
        })
    }
}

/// Bootstrap 任务执行器
pub struct BootstrapExecutor;

impl TaskExecutor for BootstrapExecutor {
    fn execute(&self, params: &Value, _ctx: &mut PipelineContext) -> Result<TaskOutput> {
        // bootstrap 通过 action 字段决定行为：env / inspect / force
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("inspect");

        let args = match action {
            "env" => crate::bootstrap::schema::Args::Env,
            "force" => crate::bootstrap::schema::Args::Force,
            _ => crate::bootstrap::schema::Args::Inspect,
        };

        crate::bootstrap::service::run(&args)?;

        Ok(TaskOutput::default())
    }
}

// ─── 任务工厂 ────────────────────────────────────────────────────────────────

/// 根据 module + action 创建对应的 TaskExecutor
pub fn create_executor(module: &str, action: Option<&str>) -> Option<Box<dyn TaskExecutor>> {
    match (module, action) {
        ("copy", _) => Some(Box::new(CopyExecutor)),
        ("scrub", _) => Some(Box::new(ScrubExecutor)),
        ("compression", Some("unzip")) => Some(Box::new(DecompressionExecutor)),
        ("compression", _) => Some(Box::new(CompressionZipExecutor)),
        ("shade", _) => Some(Box::new(ShadeExecutor)),
        ("generate", Some("path")) => Some(Box::new(GeneratePathExecutor)),
        ("generate", Some("uuid")) => Some(Box::new(GenerateUuidExecutor)),
        ("generate", Some("file")) => Some(Box::new(GenerateFileExecutor)),
        ("generate", None) => Some(Box::new(GeneratePathExecutor)),
        ("screenshot", _) => Some(Box::new(ScreenshotExecutor)),
        ("bootstrap", _) => Some(Box::new(BootstrapExecutor)),
        _ => None,
    }
}
