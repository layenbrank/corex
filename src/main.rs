use clap::Parser;
use corex::{copy, generate, setup, task, utils::notify::Notification};

#[derive(Debug, Parser)]
pub enum Commands {
    Copy(copy::controller::CopyArgs),

    #[command(subcommand)]
    Generate(generate::controller::GenerateArgs),

    #[command(subcommand)]
    Setup(setup::controller::SetupArgs),

    #[command(subcommand)]
    Task(task::controller::TaskArgs),
}

#[derive(Debug, Parser)]
#[command(
    author = "layen <15638470820@163.com>",
    version = env!("CARGO_PKG_VERSION"),
    about = "Corex 工具",
    long_about = "Corex 工具\n\n作者: layen <15638470820@163.com>"
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

fn main() {
    let args = Args::parse();

    let result = match args.command {
        Commands::Copy(args) => {
            let result = copy::service::run(&args);
            match &result {
                Ok(_) => {
                    let _ = Notification::success("复制成功", "文件复制操作已成功完成");
                }
                Err(e) => {
                    let _ =
                        Notification::error("文件复制失败", &format!("复制过程中发生错误: {}", e));
                }
            }
            result
        }
        Commands::Generate(args) => {
            generate::service::run(&args);
            Ok(())
        }
        Commands::Setup(args) => setup::service::run(&args),
        Commands::Task(args) => {
            task::controller::run(args);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("命令执行失败: {}", e);
        std::process::exit(1);
    }
}
