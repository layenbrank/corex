use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Parser)]
pub struct Args {
    from: String,

    to: String,

    description: String,
}

#[derive(Debug, Deserialize, Serialize, Parser)]
pub struct ReadArgs {
    from: String,

    description: String,
}

#[derive(Debug, Deserialize, Serialize, Parser)]
pub struct WriteArgs {
    to: String,

    description: String,
}

#[derive(Debug, Deserialize, Serialize, Parser)]
pub struct MergeArgs {
    files: Vec<String>,

    description: String,
}

#[derive(Debug, Deserialize, Serialize, Parser)]
pub struct TransformArgs {
    to: String,

    format: String,

    description: String,
}

#[derive(Debug, Deserialize, Serialize, Parser)]
pub struct SplitArgs {
    from: String,

    to: String,

    description: String,
}
