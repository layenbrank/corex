use clap::Parser;

#[derive(Debug, Parser)]
pub enum Args {
	Env,
	Check,
	Force,
}
