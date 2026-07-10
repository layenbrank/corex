//! codec 参数占位符解析

use crate::codec::schema::{
    Args, Base64Args, DecodeArgs, DecodeScheme, EncodeArgs, EncodeScheme, HashArgs, HashScheme,
    Md5Args,
};
use crate::invoke::InvokeContext;

/// 解析 codec 各子命令中的路径占位符。
pub fn parse_args(args: Args, ctx: &InvokeContext<'_>) -> Args {
    match args {
        Args::Encode(a) => Args::Encode(EncodeArgs {
            scheme: match a.scheme {
                EncodeScheme::Base64(b) => EncodeScheme::Base64(Base64Args {
                    input: b.input,
                    file: b.file.map(|p| ctx.parse(&p)),
                    output: b.output.map(|p| ctx.parse(&p)),
                }),
            },
        }),
        Args::Decode(a) => Args::Decode(DecodeArgs {
            scheme: match a.scheme {
                DecodeScheme::Base64(b) => DecodeScheme::Base64(Base64Args {
                    input: b.input,
                    file: b.file.map(|p| ctx.parse(&p)),
                    output: b.output.map(|p| ctx.parse(&p)),
                }),
            },
        }),
        Args::Hash(a) => Args::Hash(HashArgs {
            scheme: match a.scheme {
                HashScheme::Md5(b) => HashScheme::Md5(Md5Args {
                    input: b.input,
                    file: b.file.map(|p| ctx.parse(&p)),
                    output: b.output.map(|p| ctx.parse(&p)),
                }),
            },
        }),
    }
}
