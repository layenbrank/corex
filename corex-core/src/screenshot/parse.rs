//! screenshot 参数占位符解析

use crate::invoke::InvokeContext;
use crate::screenshot::schema::{Args, CaptureArgs, ClipboardArgs, CropArgs};

/// 解析 `${var.*}` / `${steps.*}` 占位符。
pub fn parse_args(parsed: Args, ctx: &InvokeContext<'_>) -> Args {
    match parsed {
        Args::Capture(args) => Args::Capture(CaptureArgs {
            to: ctx.parse(&args.to),
            description: args.description,
        }),
        Args::Crop(args) => Args::Crop(CropArgs {
            source: ctx.parse(&args.source),
            to: ctx.parse(&args.to),
            x: args.x,
            y: args.y,
            w: args.w,
            h: args.h,
            image_file: args.image_file.as_ref().map(|p| ctx.parse(p)),
            final_image_base64: args.final_image_base64,
        }),
        Args::Clipboard(args) => Args::Clipboard(ClipboardArgs {
            source: ctx.parse(&args.source),
            x: args.x,
            y: args.y,
            w: args.w,
            h: args.h,
            image_file: args.image_file.as_ref().map(|p| ctx.parse(p)),
            final_image_base64: args.final_image_base64,
        }),
        other => other,
    }
}
