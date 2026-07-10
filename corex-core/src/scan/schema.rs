use clap::Parser;
use serde::{Deserialize, Serialize};

/// scan 子命令
#[derive(Debug, Parser, Clone, Serialize, Deserialize)]
pub enum Args {
    /// 采集操作系统与硬件信息
    Os(OsArgs),
}

#[derive(Debug, Parser, Clone, Serialize, Deserialize, Default)]
pub struct OsArgs {}
