use anyhow::Result;
use serde_json::Value;

use crate::invoke::{Artifact, InvokeContext, invoke};
use crate::pipeline::context::PipelineContext;

/// Batch stage：单次 invoke，0/1 path in → artifact out
pub fn run_batch_stage(
    module: &str,
    params: &Value,
    ctx: &PipelineContext,
) -> Result<(Artifact, u64)> {
    let parsed = ctx.parse_value(params);
    let ictx = InvokeContext::pipeline(ctx);
    let result = invoke(module, parsed, &ictx)?;
    Ok((result.artifact.unwrap_or_default(), 1))
}
