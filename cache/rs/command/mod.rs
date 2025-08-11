use std::path::Path;

use crate::{copy, generate};
use clap::{ArgAction, Parser};
use serde::Deserialize;

#[derive(Parser, Debug)]
pub enum Commands {
    Copy(copy::controller::CopyArgs),

    Generate(generate::controller::GeneratePathArgs),
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
