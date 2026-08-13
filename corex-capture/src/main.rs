use anyhow::Result;
use clap::Parser;
use cx::capture;

/// 轻量 capture screenshot（等价 `corex capture screenshot --to`）
#[derive(Debug, Parser)]
#[command(version, about = "Corex 轻量 capture screenshot")]
struct Args {
    /// 截图输出目录
    #[arg(short, long)]
    to: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    capture::run(&capture::schema::Args::Screenshot(
        capture::schema::ScreenshotArgs {
            to: args.to,
            description: None,
        },
    ))
}
