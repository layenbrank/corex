use clap::{ArgAction, Parser};
use config::Config;
use fluxor::{copy, generate};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

#[derive(Parser, Debug)]
pub enum Commands {
    Copy(copy::controller::CopyArgs),
    Generate(generate::controller::GeneratePathArgs),
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
            copy::service::run(
                Path::new(&args.from),
                Path::new(&args.to),
                args.empty,
                args.ignores.clone(),
            )
            .expect("复制出错");

            println!(
                "from {} || to {} || empty {} || ignores {:?}",
                args.from, args.to, args.empty, args.ignores
            );

            // 显示完成通知
            // if let Err(e) = NotificationHelper::show_toast_notification(
            //     "Fluxor - 复制完成",
            //     &format!("文件已从 {} 复制到 {}", args.from, args.to),
            // ) {
            //     eprintln!("通知显示失败: {:?}", e);
            // }
        }
        Commands::Generate(args) => {
            println!("input {} || output {}", args.input, args.output)
        }
    }
}
