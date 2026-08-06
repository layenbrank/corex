//! morph 参数占位符解析

use crate::invoke::InvokeContext;
use crate::morph::schema::{
    Args, DocumentArgs, ExportArgs, ExtractArgs, ImagesArgs, MatchArgs, MergeArgs, MetaArgs,
    RemoveArgs, ReorderArgs, RenderArgs, RotateArgs, SplitArgs, ThumbnailsArgs,
};

/// 解析 morph 各子命令中的路径占位符。
pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    match parsed {
        Args::Meta(a) => Args::Meta(MetaArgs {
            path: ctx.parse(&a.path),
        }),
        Args::Render(a) => Args::Render(RenderArgs {
            path: ctx.parse(&a.path),
            offset: a.offset,
            scale: a.scale,
        }),
        Args::Thumbnails(a) => Args::Thumbnails(ThumbnailsArgs {
            path: ctx.parse(&a.path),
            scale: a.scale,
        }),
        Args::Match(a) => Args::Match(MatchArgs {
            path: ctx.parse(&a.path),
            query: a.query,
        }),
        Args::Export(a) => Args::Export(ExportArgs {
            src: ctx.parse(&a.src),
            dest: ctx.parse(&a.dest),
        }),
        Args::Merge(a) => Args::Merge(MergeArgs {
            paths: a.paths.iter().map(|p| ctx.parse(p)).collect(),
            dest: ctx.parse(&a.dest),
        }),
        Args::Split(a) => Args::Split(SplitArgs {
            path: ctx.parse(&a.path),
            ranges: a.ranges,
            limit: a.limit,
            dir: ctx.parse(&a.dir),
        }),
        Args::Images(a) => Args::Images(ImagesArgs {
            path: ctx.parse(&a.path),
            scale: a.scale,
            format: a.format,
            dir: ctx.parse(&a.dir),
        }),
        Args::Document(a) => Args::Document(DocumentArgs {
            path: ctx.parse(&a.path),
            format: a.format,
            dir: ctx.parse(&a.dir),
        }),
        Args::Reorder(a) => Args::Reorder(ReorderArgs {
            path: ctx.parse(&a.path),
            order: a.order,
            dest: ctx.parse(&a.dest),
        }),
        Args::Rotate(a) => Args::Rotate(RotateArgs {
            path: ctx.parse(&a.path),
            pages: a.pages,
            degrees: a.degrees,
            dest: ctx.parse(&a.dest),
        }),
        Args::Remove(a) => Args::Remove(RemoveArgs {
            path: ctx.parse(&a.path),
            pages: a.pages,
            dest: ctx.parse(&a.dest),
        }),
        Args::Extract(a) => Args::Extract(ExtractArgs {
            path: ctx.parse(&a.path),
            pages: a.pages,
            dest: ctx.parse(&a.dest),
        }),
    }
}
