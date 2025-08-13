use clap::Parser;
// use config::Config;
use fluxor::{copy, generate};
// use serde::Deserialize;
// use std::{collections::HashMap};
use std::path::Path;

#[derive(Debug, Parser)]
pub enum Commands {
    Copy(copy::controller::CopyArgs),

    #[command(subcommand)]
    Generate(generate::controller::GenerateArgs),
}

#[derive(Debug, Parser)]
pub struct Args {
    #[arg(short, long, help = "Enable verbose mode")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

fn main() {
    // let settings = Config::builder()
    //     .add_source(config::File::with_name("config.json"))
    //     .add_source(config::Environment::with_prefix("config"))
    //     .build()
    //     .expect("配置文件获取失败");

    // let deserialize = settings
    //     .try_deserialize::<HashMap<String, CopyArgs>>()
    //     .expect("转换失败");

    // println!("deserialize {:?}", deserialize);

    let args = Args::parse();

    match args.command {
        Commands::Copy(args) => {
            copy::service::run(&args).expect("复制出错");

            println!(
                "from: {}\nto: {}\nempty: {}\nignores: {:?}",
                args.from, args.to, args.empty, args.ignores
            );
        }
        Commands::Generate(generate_args) => {
            match generate_args {
                generate::controller::GenerateArgs::Path(args) => {
                    println!(
                        "from: {:?}\nto: {:?}",
                        Path::new(&args.from),
                        Path::new(&args.to)
                    );
                    generate::service::run_path(&args).expect("生成路径出错");
                    // 这里添加您的 generate 逻辑
                }
            }
        }
    }
}
