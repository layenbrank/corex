use clap::{ArgAction, Parser};

#[derive(Debug, Parser)]
pub struct SetupArgs {
    #[arg(long, action = ArgAction::SetTrue, help = "启用详细模式")]
    pub verbose: bool,

    /// 是否启用环境设置
    #[arg(long, action = ArgAction::SetTrue, help = "启用环境设置")]
    pub env: bool,

    /// 是否仅检查状态而不执行初始化
    #[arg(long, action = ArgAction::SetTrue, help = "检查环境配置")]
    pub check: bool,

    /// 是否强制重新初始化（即使已经在PATH中）
    #[arg(long, action = ArgAction::SetTrue, help = "强制执行环境设置")]
    pub force: bool,
}
