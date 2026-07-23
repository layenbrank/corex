//! codec 参数占位符解析

use crate::codec::schema::{
    Args, Base64Args, DecodeAlgorithm, DecodeArgs, EncodeAlgorithm, EncodeArgs, HashAlgorithm,
    HashArgs, Md5Args,
};
use crate::invoke::InvokeContext;

/// 解析 codec 各子命令中的路径占位符。
pub fn parse_args(args: Args, ctx: &InvokeContext<'_>) -> Args {
    match args {
        Args::Encode(a) => Args::Encode(EncodeArgs {
            algorithm: match a.algorithm {
                EncodeAlgorithm::Base64(b) => EncodeAlgorithm::Base64(Base64Args {
                    input: b.input,
                    file: b.file.map(|p| ctx.parse(&p)),
                    output: b.output.map(|p| ctx.parse(&p)),
                }),
            },
        }),
        Args::Decode(a) => Args::Decode(DecodeArgs {
            algorithm: match a.algorithm {
                DecodeAlgorithm::Base64(b) => DecodeAlgorithm::Base64(Base64Args {
                    input: b.input,
                    file: b.file.map(|p| ctx.parse(&p)),
                    output: b.output.map(|p| ctx.parse(&p)),
                }),
            },
        }),
        Args::Hash(a) => Args::Hash(HashArgs {
            algorithm: match a.algorithm {
                HashAlgorithm::Md5(b) => HashAlgorithm::Md5(Md5Args {
                    input: b.input,
                    file: b.file.map(|p| ctx.parse(&p)),
                    output: b.output.map(|p| ctx.parse(&p)),
                }),
            },
        }),
    }
}
