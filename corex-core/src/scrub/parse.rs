//! scrub 参数占位符解析

use crate::invoke::InvokeContext;
use crate::scrub::schema::Args;

/// 解析 `${var.*}` / `${steps.*}` 占位符。
pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    Args {
        source: ctx.parse(&parsed.source),
        target: parsed.target,
        recursive: parsed.recursive,
        description: parsed.description,
    }
}
