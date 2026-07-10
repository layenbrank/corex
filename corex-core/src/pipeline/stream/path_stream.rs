//! generate Path 流式 stage

use anyhow::Result;

use crate::generate::schema::PathArgs;
use crate::invoke::Artifact;

/// 流式执行 generate Path
pub fn run_path_stream(args: &PathArgs) -> Result<(Artifact, u64)> {
    let output =
        crate::generate::service::execute(&crate::generate::schema::Args::Path(args.clone()))?;
    Ok((
        Artifact::from_path(output.path.unwrap_or_default()),
        output.items,
    ))
}

/// 同步别名
pub fn run_path_stream_blocking(args: &PathArgs) -> Result<(Artifact, u64)> {
    run_path_stream(args)
}
