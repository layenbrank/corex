use clap::Parser;
use corex_lib::{bootstrap, compression, copy, generate, schedule, scrub};

#[derive(Debug, Parser)]
pub enum Commands {
    Copy(copy::controller::Args),
    Scrub(scrub::controller::Args),

    #[command(subcommand)]
    Generate(generate::controller::Args),

    #[command(subcommand)]
    Bootstrap(bootstrap::controller::Args),

    #[command(subcommand)]
    Schedule(schedule::controller::Args),

    Compression(compression::controller::Args),
}

#[derive(Debug, Parser)]
#[command(
	author = "layen <15638470820@163.com>",
	version = env!("CARGO_PKG_VERSION"),
	about = "Corex Tools",
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[tokio::main]
async fn main() {
    match Args::parse().command {
        Commands::Schedule(args) => schedule::service::run(&args),
        Commands::Copy(args) => copy::service::run(&args),
        Commands::Scrub(args) => scrub::service::run(&args),
        Commands::Generate(args) => generate::service::run(&args),
        Commands::Bootstrap(args) => bootstrap::service::run(&args),
        Commands::Compression(args) => compression::service::run(&args),
    }
}
