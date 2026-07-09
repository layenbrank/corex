use clap::Parser;
use cx::command::{Args, dispatch};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    dispatch(args)
}
