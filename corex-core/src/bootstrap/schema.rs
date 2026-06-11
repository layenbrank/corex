use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Parser, Serialize, Deserialize)]
pub enum Args {
    Env,
    Inspect,
    Force,
}
