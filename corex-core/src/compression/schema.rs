use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::utils::verifier;

#[derive(Parser, Debug, Clone, Deserialize, Serialize)]
pub struct Args {
    #[arg(short, long, value_parser = verifier::path)]
    pub from: String,

    #[arg(short, long)]
    pub to: String,

    #[arg(help = "任务描述")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    #[arg(help = "任务ID")]
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
}
