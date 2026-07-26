//! engine 参数占位符解析

use crate::engine::schema::{Args, SuggestionArgs};
use crate::invoke::InvokeContext;

/// 解析 `${var.*}` / `${steps.*}` 占位符。
pub fn parse_args(args: Args, ctx: &InvokeContext<'_>) -> Args {
    match args {
        Args::Suggestion(a) => Args::Suggestion(SuggestionArgs {
            pt: ctx.parse(&a.pt),
            qry: ctx.parse(&a.qry),
            cp: a.cp,
            csr: ctx.parse(&a.csr),
            pths: ctx.parse(&a.pths),
            cvid: a.cvid.map(|v| ctx.parse(&v)),
            user_agent: a.user_agent.map(|v| ctx.parse(&v)),
        }),
    }
}
