use anyhow::Result;
use clap::Parser;

use crate::bootstrap;
use crate::compression;
use crate::copy;
use crate::generate;
use crate::pipeline;
use crate::screenshot;
use crate::scrub;

/// Corex 命令行入口
#[derive(Debug, Parser)]
#[command(
    author = "layen <15638470820@163.com>",
    version = env!("CARGO_PKG_VERSION"),
    about = "Corex Tools — 多功能 CLI 工具",
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// 复制文件或目录
    Copy(copy::schema::Args),
    /// 清理指定名称的文件/目录
    Scrub(scrub::schema::Args),
    /// 生成路径列表或 UUID
    #[command(subcommand)]
    Generate(generate::schema::Args),
    /// 环境初始化与检查
    #[command(subcommand)]
    Bootstrap(bootstrap::schema::Args),
    /// 截图
    Screenshot(screenshot::schema::Args),
    /// 压缩打包
    Compression(compression::schema::Args),
    /// 执行 Pipeline
    Pipeline(pipeline::config::PipelineArgs),
    /// 任务调度器
    #[command(subcommand)]
    Schedule(pipeline::config::ScheduleArgs),
}

/// 分发命令到对应处理器
pub fn dispatch(args: Args) -> Result<()> {
    match args.command {
        Commands::Copy(a) => copy::service::run(&a),
        Commands::Scrub(a) => scrub::service::run(&a),
        Commands::Schedule(a) => pipeline::runner::run_schedule(&a),
        Commands::Generate(a) => generate::service::run(&a),
        Commands::Bootstrap(a) => bootstrap::service::run(&a),
        Commands::Screenshot(a) => screenshot::service::run(&a),
        Commands::Compression(a) => compression::service::run(&a),
        Commands::Pipeline(a) => pipeline::runner::run_pipeline_cmd(&a),
    }
}
