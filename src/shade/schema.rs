use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Parser)]
pub struct Args {
    from: String,

    to: String,

    format: String,

    quality: u8,

    description: String,
}
