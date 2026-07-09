use anyhow::Result;
use clap::Parser;
use cx::screenshot;

/// 轻量截图工具（仅含 xcap，无完整 corex 依赖）
#[derive(Debug, Parser)]
#[command(version, about = "Corex 轻量截图工具")]
struct Args {
    /// 截图输出目录
    #[arg(short, long)]
    to: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    screenshot::run(&screenshot::schema::Args {
        to: args.to,
        description: None,
    })
}
