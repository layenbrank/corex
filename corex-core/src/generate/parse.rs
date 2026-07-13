//! generate 参数占位符解析

use crate::generate::schema::{Args, PathArgs};
use crate::invoke::InvokeContext;

/// 解析 `${var.*}` / `${steps.*}` 占位符。
pub fn parse_args(args: Args, ctx: &InvokeContext<'_>) -> Args {
    match args {
        Args::Path(a) => Args::Path(PathArgs {
            from: ctx.parse(&a.from),
            to: ctx.parse(&a.to),
            transform: a.transform,
            index: a.index,
            separator: a.separator,
            pad: a.pad,
            includes: a.includes,
            excludes: a.excludes,
            uppercase: a.uppercase,
            id: a.id,
            description: a.description,
        }),
        Args::Uuid(a) => Args::Uuid(a),
    }
}
