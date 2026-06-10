use clap::Parser;
use corex_lib::{bootstrap, compression, copy, generate, schedule, screenshot, scrub};

#[derive(Debug, Parser)]
pub enum Commands {
    Copy(copy::schema::Args),
    Scrub(scrub::schema::Args),

    #[command(subcommand)]
    Generate(generate::schema::Args),

    #[command(subcommand)]
    Bootstrap(bootstrap::schema::Args),

    #[command(subcommand)]
    Schedule(schedule::schema::Args),

    Screenshot(screenshot::schema::Args),

    Compression(compression::schema::Args),
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

fn main() -> anyhow::Result<()> {
    match Args::parse().command {
        Commands::Copy(args) => copy::service::run(&args)?,
        Commands::Scrub(args) => scrub::service::run(&args)?,
        Commands::Schedule(args) => schedule::service::run(&args)?,
        Commands::Generate(args) => generate::service::run(&args)?,
        Commands::Bootstrap(args) => bootstrap::service::run(&args)?,
        Commands::Screenshot(args) => screenshot::service::run(&args)?,
        Commands::Compression(args) => compression::service::run(&args)?,
    }
    Ok(())
}
