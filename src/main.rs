use clap::Parser;
use config::Config;
use rust_demo::{process_copy, Args, Commands, CopyArgs};
use std::{collections::HashMap, path::Path};

fn main() {
    let settings = Config::builder()
        .add_source(config::File::with_name("config.json"))
        .add_source(config::Environment::with_prefix("config"))
        .build()
        .expect("配置文件获取失败");

    let deserialize = settings
        .try_deserialize::<HashMap<String, CopyArgs>>()
        .expect("转换失败");

    println!("deserialize {:?}", deserialize);

    let args = Args::parse();

    match args.command {
        Commands::Copy(args) => {
            process_copy(
                Path::new(&args.from),
                Path::new(&args.to),
                args.ignores.clone(),
            )
            .expect("复制出错");

            println!(
                "from {} || to {} || ignores {:?}",
                args.from, args.to, args.ignores
            );
        }
        Commands::Path(args) => {
            println!("input {} || output {}", args.input, args.output)
        }
    }
}
