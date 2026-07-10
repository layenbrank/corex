//! copy 参数占位符解析

use crate::copy::schema::Args;
use crate::invoke::InvokeContext;

/// 解析 `${var.*}` / `${steps.*}` 占位符。
pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    Args {
        from: ctx.parse(&parsed.from),
        to: ctx.parse(&parsed.to),
        empty: parsed.empty,
        includes: parsed.includes,
        excludes: parsed.excludes,
        id: parsed.id,
        description: parsed.description,
    }
}
