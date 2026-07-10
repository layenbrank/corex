//! generate 参数占位符解析

use crate::generate::schema::{Args, FileArgs, PathArgs};
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
        Args::File(a) => Args::File(FileArgs {
            to: ctx.parse(&a.to),
            template: a.template.as_ref().map(|s| ctx.parse(s)),
            fragment: a.fragment.as_ref().map(|s| ctx.parse(s)),
            variable: a
                .variable
                .iter()
                .map(|(k, v)| (k.clone(), ctx.parse(v)))
                .collect(),
            id: a.id,
            description: a.description,
        }),
    }
}
