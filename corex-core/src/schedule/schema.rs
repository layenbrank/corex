use clap::Parser;

/// `corex schedule` 子命令参数
#[derive(Debug, Clone, Parser)]
pub enum Args {
    /// 交互式选择并执行 Pipeline
    Run,
    /// 生成配置文件模板
    Generate,
    /// 以守护进程模式运行（按 cron 表达式定时执行）
    Cron {
        /// 配置文件路径
        #[arg(short, long)]
        config: Option<String>,
        /// 仅调度指定 pipeline id（可多次指定）
        #[arg(short, long)]
        pipeline: Vec<String>,
    },
}
