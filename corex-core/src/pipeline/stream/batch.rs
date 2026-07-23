use anyhow::Result;

use crate::invoke::{Artifact, InvokeContext, WireArgs, invoke};
use crate::pipeline::config::StepConfig;
use crate::pipeline::context::PipelineContext;

/// Batch stage：单次 invoke，0/1 path in → artifact out
pub fn run_batch_stage(step: &StepConfig, ctx: &PipelineContext) -> Result<(Artifact, u64)> {
    let parsed = ctx.parse_value(&step.params);
    let wire = WireArgs {
        action: step.action.clone(),
        format: step.format.clone(),
        algorithm: step.algorithm.clone(),
        flags: parsed,
    };
    let ictx = InvokeContext::pipeline(ctx);
    let result = invoke(&step.module, wire, &ictx)?;
    Ok((result.artifact.unwrap_or_default(), 1))
}
