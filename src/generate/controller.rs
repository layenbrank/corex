use crate::utils::verifier::Verifier;
use clap::Parser;

#[derive(Debug, Parser)]
pub struct GeneratePathArgs {
    #[arg(short, long, value_parser = Verifier::path)]
    pub input: String,

    #[arg(short, long, value_parser = Verifier::path)]
    pub output: String,
    // #[arg(short, long)]
    // pub recursive: bool,

    // #[arg(short, long)]
    // pub separator: String,

    // #[arg(short, long)]
    // pub transform: String,
}
