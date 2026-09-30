//! 指令库的错误。

use corex_core::EngineError;
use thiserror::Error;

/// 指令库出错的几种样子。
///
/// [`StoreError::kind`] 与 [`EngineError::kind`] 用同一套词汇：CLI 退出码、daemon 的
/// IPC 错误码都按它分流，多一种错误不必在多处各补一条 `match`。
#[derive(Debug, Error)]
pub enum StoreError {
    /// 库里没有这条指令。
    #[error("指令未找到: {0}")]
    NotFound(String),

    /// 名字被占着：改名或导入撞上已有条目。
    #[error("指令已存在: {0}")]
    NameTaken(String),

    /// 名字不是裸名（含分隔符、`..`、盘符或绝对路径）。
    #[error("非法指令名: {0}")]
    InvalidName(String),

    /// 存不住的东西：YAML 解析失败，或库里的 JSON 读不成模型。
    #[error("指令定义不合法: {0}")]
    Invalid(String),

    #[error("指令库读写失败: {0}")]
    Sql(#[from] rusqlite::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

impl StoreError {
    /// 机器可读的类别（与 [`EngineError::kind`] 同一套词汇）。
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::NameTaken(_) => "conflict",
            Self::InvalidName(_) => "usage",
            Self::Invalid(_) => "parse",
            Self::Sql(_) | Self::Io(_) => "io",
        }
    }
}

/// 引擎侧的「找不到 / 解析失败」直接沿用同一档错误码，不必在库里再造一套。
impl From<EngineError> for StoreError {
    fn from(error: EngineError) -> Self {
        match error {
            EngineError::DirectiveNotFound(name) => Self::NotFound(name),
            EngineError::ParseError(message) => Self::Invalid(message),
            other => Self::Invalid(other.to_string()),
        }
    }
}

impl From<StoreError> for EngineError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound(name) => Self::DirectiveNotFound(name),
            StoreError::NameTaken(name) => Self::Usage(format!("指令已存在: {name}")),
            StoreError::InvalidName(name) => Self::Usage(format!("非法指令名: {name}")),
            StoreError::Invalid(message) => Self::ParseError(message),
            other => Self::Other(other.to_string()),
        }
    }
}
