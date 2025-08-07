use std::path::Path;

use clap::{ArgAction, Parser};
use serde::Deserialize;

mod copy;

pub use copy::CopyArgs;

#[derive(Debug, Parser)]
pub struct GeneratePathArgs {
    #[arg(short, long, value_parser = verify_path)]
    pub input: String,

    #[arg(short, long, value_parser = verify_path)]
    pub output: String,
    // #[arg(short, long)]
    // pub recursive: bool,

    // #[arg(short, long)]
    // pub separator: String,

    // #[arg(short, long)]
    // pub transform: String,
}

#[derive(Parser, Debug)]
pub enum Commands {
    Copy(CopyArgs),

    Path(GeneratePathArgs),
}

#[derive(Debug, Parser)]
pub struct Args {
    #[arg(short, long, help = "Enable verbose mode")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

pub fn verify_path(path: &str) -> Result<String, &'static str> {
    if path == "." || Path::new(&path).exists() {
        println!("{}", path);
        Ok(path.into())
    } else {
        Err("未找到指定路径，请检查路径是否正确！")
    }
}
