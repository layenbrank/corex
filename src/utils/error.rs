use std::fmt;

/// 统一的错误类型
#[derive(Debug)]
pub enum CorexError {
    /// IO 错误
    IoError(std::io::Error),
    /// 路径相关错误
    PathError(String),
    /// 配置错误
    ConfigError(String),
    /// 验证错误
    ValidationError(String),
    /// 通知错误
    NotificationError(String),
    /// 其他错误
    Other(String),
}

impl fmt::Display for CorexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CorexError::IoError(err) => write!(f, "IO错误: {}", err),
            CorexError::PathError(msg) => write!(f, "路径错误: {}", msg),
            CorexError::ConfigError(msg) => write!(f, "配置错误: {}", msg),
            CorexError::ValidationError(msg) => write!(f, "验证错误: {}", msg),
            CorexError::NotificationError(msg) => write!(f, "通知错误: {}", msg),
            CorexError::Other(msg) => write!(f, "错误: {}", msg),
        }
    }
}

impl std::error::Error for CorexError {}

impl From<std::io::Error> for CorexError {
    fn from(err: std::io::Error) -> Self {
        CorexError::IoError(err)
    }
}

impl From<anyhow::Error> for CorexError {
    fn from(err: anyhow::Error) -> Self {
        CorexError::Other(err.to_string())
    }
}

/// 统一的结果类型
pub type CorexResult<T> = Result<T, CorexError>;
