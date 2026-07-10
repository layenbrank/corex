use anyhow::Result;
use clap::Parser;
use cx::serve::{ServeOptions, run};

/// Corex 长驻 Daemon — 通过 Named Pipe 接收 JSON 请求
#[derive(Debug, Parser)]
#[command(version, about = "Corex IPC Daemon (Named Pipe)")]
struct Args {
    /// Named Pipe 路径
    #[arg(long, default_value = r"\\.\pipe\corex")]
    pipe: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    run(ServeOptions {
        pipe_name: args.pipe,
    })
}
