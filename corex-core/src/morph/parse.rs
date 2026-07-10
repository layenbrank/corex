//! morph 参数占位符解析

use crate::invoke::InvokeContext;
use crate::morph::schema::{
    Args, ExportArgs, MergeArgs, MetaArgs, RenderPageArgs, RenderThumbnailsArgs, SearchArgs,
    SplitArgs, SplitByCountArgs, ToImagesArgs, ToOfficeArgs,
};

/// 解析 morph 各子命令中的路径占位符。
pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    match parsed {
        Args::Meta(a) => Args::Meta(MetaArgs {
            path: ctx.parse(&a.path),
        }),
        Args::RenderPage(a) => Args::RenderPage(RenderPageArgs {
            path: ctx.parse(&a.path),
            page_index: a.page_index,
            scale: a.scale,
        }),
        Args::RenderThumbnails(a) => Args::RenderThumbnails(RenderThumbnailsArgs {
            path: ctx.parse(&a.path),
            scale: a.scale,
        }),
        Args::Search(a) => Args::Search(SearchArgs {
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
            dest_dir: ctx.parse(&a.dest_dir),
        }),
        Args::SplitByCount(a) => Args::SplitByCount(SplitByCountArgs {
            path: ctx.parse(&a.path),
            pages_per_file: a.pages_per_file,
            dest_dir: ctx.parse(&a.dest_dir),
        }),
        Args::ToImages(a) => Args::ToImages(ToImagesArgs {
            path: ctx.parse(&a.path),
            scale: a.scale,
            format: a.format,
            dest_dir: ctx.parse(&a.dest_dir),
        }),
        Args::ToOffice(a) => Args::ToOffice(ToOfficeArgs {
            path: ctx.parse(&a.path),
            format: a.format,
            dest_dir: ctx.parse(&a.dest_dir),
        }),
    }
}
