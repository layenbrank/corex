use anyhow::Result;
use clap::Parser;
use cx::screenshot;

/// 轻量截图 capture（等价 `corex screenshot capture --to`）
#[derive(Debug, Parser)]
#[command(version, about = "Corex 轻量截图 capture")]
struct Args {
    /// 截图输出目录
    #[arg(short, long)]
    to: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    screenshot::run(&screenshot::schema::Args::Capture(
        screenshot::schema::CaptureArgs {
            to: args.to,
            description: None,
        },
    ))
}
