use clap::Parser;

#[derive(Debug, Parser)]
pub enum SetupArgs {
    Env,
    Check,
    Force,
}
