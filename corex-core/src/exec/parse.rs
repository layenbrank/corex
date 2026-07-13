//! exec 参数占位符解析

use crate::exec::schema::{Args, RunArgs};
use crate::invoke::InvokeContext;

/// 解析 `${var.*}` / `${steps.*}` 占位符。
pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    match parsed {
        Args::Run(a) => Args::Run(RunArgs {
            script: ctx.parse(&a.script),
            args: a.args.iter().map(|arg| ctx.parse(arg)).collect(),
            cwd: a.cwd.as_ref().map(|s| ctx.parse(s)),
            capture: a.capture,
        }),
    }
}
