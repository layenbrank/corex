// use std::fs;

use std::path::Path;

use clap::Parser;

use rust_demo::{process_copy, Args, Commands};

// #[derive(Parser, Debug)]
// #[command(version, about)]
// struct Args {
//     #[arg(short, long)]
//     copy: String,

//     #[arg(short, long)]
//     generate_path: String,
// }

fn main() {
    let args = Args::parse();

    // println!("{:?}", args);

    match args.command {
        Commands::CopyPlugin(args) => {
            process_copy(Path::new(&args.from), Path::new(&args.to), args.ignores)
                .expect("复制出错");

            println!("from {} || to {}", args.from, args.to)
        }
        Commands::GeneratePath(args) => {
            println!("input {} || output {}", args.input, args.output)
        }
    }
}
