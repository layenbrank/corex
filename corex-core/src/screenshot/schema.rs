use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

#[derive(Debug, Deserialize, Serialize, Parser)]
pub struct Args {
    #[arg(short, long, value_parser = verifier::path)]
    pub to: String,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,
}
