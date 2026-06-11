use clap::Parser;
use corex_core::cli::{Args, dispatch};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    dispatch(args)
}
