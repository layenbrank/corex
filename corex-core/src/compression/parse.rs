//! compression 参数占位符解析

use crate::compression::schema::{
    ArchiveIoArgs, Args, CompressFormat, DecompressFormat, SevenZDecompressArgs, SevenZFormatArgs,
    TarGzDecompressArgs, TarGzFormatArgs, ZipDecompressArgs, ZipFormatArgs,
};
use crate::invoke::InvokeContext;

/// 解析 compression 各格式中的路径 / 密码占位符。
pub fn parse_args(args: Args, ctx: &InvokeContext<'_>) -> Args {
    match args {
        Args::Compress(c) => Args::Compress(crate::compression::schema::CompressArgs {
            format: match c.format {
                CompressFormat::Zip(z) => CompressFormat::Zip(parse_zip_compress(z, ctx)),
                CompressFormat::TarGz(t) => CompressFormat::TarGz(parse_tar_gz_compress(t, ctx)),
                CompressFormat::SevenZ(s) => CompressFormat::SevenZ(parse_seven_z_compress(s, ctx)),
            },
        }),
        Args::Decompress(d) => Args::Decompress(crate::compression::schema::DecompressArgs {
            format: match d.format {
                DecompressFormat::Zip(z) => DecompressFormat::Zip(parse_zip_decompress(z, ctx)),
                DecompressFormat::TarGz(t) => {
                    DecompressFormat::TarGz(parse_tar_gz_decompress(t, ctx))
                }
                DecompressFormat::SevenZ(s) => {
                    DecompressFormat::SevenZ(parse_seven_z_decompress(s, ctx))
                }
            },
        }),
    }
}

fn parse_io(io: ArchiveIoArgs, ctx: &InvokeContext<'_>) -> ArchiveIoArgs {
    ArchiveIoArgs {
        includes: io.includes,
        excludes: io.excludes,
        password: io.password.map(|pw| ctx.parse(&pw)),
        overwrite: io.overwrite,
        preserve_timestamps: io.preserve_timestamps,
    }
}

fn parse_zip_compress(args: ZipFormatArgs, ctx: &InvokeContext<'_>) -> ZipFormatArgs {
    ZipFormatArgs {
        from: ctx.parse(&args.from),
        to: ctx.parse(&args.to),
        level: args.level,
        method: args.method,
        encryption: args.encryption,
        io: parse_io(args.io, ctx),
        description: args.description,
        id: args.id,
    }
}

fn parse_zip_decompress(args: ZipDecompressArgs, ctx: &InvokeContext<'_>) -> ZipDecompressArgs {
    ZipDecompressArgs {
        from: ctx.parse(&args.from),
        to: ctx.parse(&args.to),
        io: parse_io(args.io, ctx),
        description: args.description,
        id: args.id,
    }
}

fn parse_tar_gz_compress(args: TarGzFormatArgs, ctx: &InvokeContext<'_>) -> TarGzFormatArgs {
    TarGzFormatArgs {
        from: ctx.parse(&args.from),
        to: ctx.parse(&args.to),
        level: args.level,
        preserve_permissions: args.preserve_permissions,
        io: parse_io(args.io, ctx),
        description: args.description,
        id: args.id,
    }
}

fn parse_tar_gz_decompress(
    args: TarGzDecompressArgs,
    ctx: &InvokeContext<'_>,
) -> TarGzDecompressArgs {
    TarGzDecompressArgs {
        from: ctx.parse(&args.from),
        to: ctx.parse(&args.to),
        io: parse_io(args.io, ctx),
        description: args.description,
        id: args.id,
    }
}

fn parse_seven_z_compress(args: SevenZFormatArgs, ctx: &InvokeContext<'_>) -> SevenZFormatArgs {
    SevenZFormatArgs {
        from: ctx.parse(&args.from),
        to: ctx.parse(&args.to),
        level: args.level,
        solid: args.solid,
        encrypt_header: args.encrypt_header,
        io: parse_io(args.io, ctx),
        description: args.description,
        id: args.id,
    }
}

fn parse_seven_z_decompress(
    args: SevenZDecompressArgs,
    ctx: &InvokeContext<'_>,
) -> SevenZDecompressArgs {
    SevenZDecompressArgs {
        from: ctx.parse(&args.from),
        to: ctx.parse(&args.to),
        io: parse_io(args.io, ctx),
        description: args.description,
        id: args.id,
    }
}
