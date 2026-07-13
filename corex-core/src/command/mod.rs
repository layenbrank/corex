use anyhow::Result;
use clap::Parser;

use crate::runtime::RuntimeOpts;

#[cfg(feature = "bootstrap")]
use crate::bootstrap;
#[cfg(feature = "codec")]
use crate::codec;
#[cfg(feature = "compression")]
use crate::compression;
#[cfg(feature = "copy")]
use crate::copy;
#[cfg(feature = "exec")]
use crate::exec;
#[cfg(feature = "generate")]
use crate::generate;
#[cfg(feature = "morph")]
use crate::morph;
#[cfg(feature = "pipeline")]
use crate::pipeline;
#[cfg(feature = "scan")]
use crate::scan;
#[cfg(feature = "schedule")]
use crate::schedule;
#[cfg(feature = "watch")]
use crate::watch;
#[cfg(feature = "screenshot")]
use crate::screenshot;
#[cfg(feature = "scrub")]
use crate::scrub;
#[cfg(feature = "shade")]
use crate::shade;

/// Corex 命令行入口
#[derive(Debug, Parser)]
#[command(
    author = "layen <15638470820@163.com>",
    version = env!("CARGO_PKG_VERSION"),
    about = "Corex Tools — 多功能 CLI 工具",
)]
pub struct Args {
    #[command(flatten)]
    pub runtime: RuntimeOpts,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Parser)]
pub enum Commands {
    /// 编解码与摘要
    #[cfg(feature = "codec")]
    #[command(subcommand)]
    Codec(codec::schema::Args),
    /// 系统信息采集
    #[cfg(feature = "scan")]
    #[command(subcommand)]
    Scan(scan::schema::Args),
    /// PDF 处理
    #[cfg(feature = "morph")]
    #[command(subcommand)]
    Morph(morph::schema::Args),
    /// 复制文件或目录
    #[cfg(feature = "copy")]
    Copy(copy::schema::Args),
    /// 清理指定名称的文件/目录
    #[cfg(feature = "scrub")]
    Scrub(scrub::schema::Args),
    /// 生成路径列表或 UUID
    #[cfg(feature = "generate")]
    #[command(subcommand)]
    Generate(generate::schema::Args),
    /// 运行外部脚本
    #[cfg(feature = "exec")]
    #[command(subcommand)]
    Exec(exec::schema::Args),
    /// 环境初始化与检查
    #[cfg(feature = "bootstrap")]
    #[command(subcommand)]
    Bootstrap(bootstrap::schema::Args),
    /// 截图
    #[cfg(feature = "screenshot")]
    #[command(subcommand)]
    Screenshot(screenshot::schema::Args),
    /// 压缩打包 / 解压缩
    #[cfg(feature = "compression")]
    #[command(subcommand)]
    Compression(compression::schema::Args),
    /// 图片处理（格式转换 / 无损压缩）
    #[cfg(feature = "shade")]
    Shade(shade::schema::Args),
    /// 执行 Pipeline
    #[cfg(feature = "pipeline")]
    Pipeline(pipeline::config::PipelineArgs),
    /// 任务调度器
    #[cfg(feature = "schedule")]
    #[command(subcommand)]
    Schedule(schedule::schema::Args),
    /// 文件变更监听
    #[cfg(feature = "watch")]
    #[command(subcommand)]
    Watch(watch::schema::Args),
}

/// 分发命令到对应处理器
pub fn dispatch(args: Args) -> Result<()> {
    match args.command {
        #[cfg(feature = "codec")]
        Commands::Codec(a) => codec::run(&a),
        #[cfg(feature = "scan")]
        Commands::Scan(a) => scan::run(&a),
        #[cfg(feature = "morph")]
        Commands::Morph(a) => morph::run(&a),
        #[cfg(feature = "copy")]
        Commands::Copy(a) => copy::run(&a),
        #[cfg(feature = "scrub")]
        Commands::Scrub(a) => scrub::run(&a),
        #[cfg(feature = "shade")]
        Commands::Shade(a) => shade::run(&a),
        #[cfg(feature = "schedule")]
        Commands::Schedule(a) => schedule::run(&a),
        #[cfg(feature = "watch")]
        Commands::Watch(a) => watch::run(&a),
        #[cfg(feature = "generate")]
        Commands::Generate(a) => generate::run(&a),
        #[cfg(feature = "exec")]
        Commands::Exec(a) => exec::run(&a),
        #[cfg(feature = "bootstrap")]
        Commands::Bootstrap(a) => bootstrap::run(&a),
        #[cfg(feature = "screenshot")]
        Commands::Screenshot(a) => screenshot::run(&a),
        #[cfg(feature = "compression")]
        Commands::Compression(a) => compression::run(&a),
        #[cfg(feature = "pipeline")]
        Commands::Pipeline(a) => pipeline::run(&a),
    }
}
