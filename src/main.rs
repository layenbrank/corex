use std::{collections::HashMap, path::Path};

use clap::Parser;
use config::Config;

use rust_demo::{process_copy, Args, Commands, CopyArgs};

fn main() {
    let settings = Config::builder()
        .add_source(config::File::with_name("config.json"))
        .add_source(config::Environment::with_prefix("config"))
        .build()
        .expect("配置文件获取失败");

    let deserialize = settings
        .try_deserialize::<HashMap<String, CopyArgs>>()
        .expect("转换失败");

    println!("{:?}", deserialize);

    let args = Args::parse();

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
