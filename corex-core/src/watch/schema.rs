use clap::{ArgAction, Parser};

/// `corex watch` 子命令参数
#[derive(Debug, Clone, Parser)]
pub enum Args {
    /// 启动文件监听守护进程
    Run {
        /// 配置文件路径
        #[arg(short, long)]
        config: Option<String>,
        /// 仅监听指定 pipeline id（可多次指定）
        #[arg(short, long)]
        pipeline: Vec<String>,
        /// 覆盖 debounce 毫秒（对所有 pipeline 生效）
        #[arg(long)]
        debounce_ms: Option<u64>,
        /// 追加 glob 白名单（与 yaml includes 合并）
        #[arg(long, action = ArgAction::Append, value_delimiter = ',')]
        includes: Vec<String>,
        /// 追加 glob 黑名单（与 yaml excludes 合并）
        #[arg(long, action = ArgAction::Append, value_delimiter = ',')]
        excludes: Vec<String>,
        /// 启动后立即执行一次
        #[arg(long)]
        immediate: bool,
    },
}
