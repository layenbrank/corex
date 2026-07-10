//! shade 参数占位符解析

use crate::invoke::InvokeContext;
use crate::shade::schema::Args;

/// 解析 `${var.*}` / `${steps.*}` 占位符。
pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    Args {
        from: ctx.parse(&parsed.from),
        to: ctx.parse(&parsed.to),
        format: parsed.format,
        quality: parsed.quality,
        description: parsed.description,
        id: parsed.id,
    }
}
