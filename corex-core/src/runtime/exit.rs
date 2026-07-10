use thiserror::Error;

/// 标准化进程退出码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitStatus {
    Success = 0,
    Usage = 1,
    Config = 2,
    Runtime = 3,
    Io = 4,
    Internal = 5,
}

impl ExitStatus {
    pub fn code(self) -> i32 {
        self as i32
    }
}

impl std::process::Termination for ExitStatus {
    fn report(self) -> std::process::ExitCode {
        std::process::ExitCode::from(self as u8)
    }
}

/// 应用级错误（映射到退出码）
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Runtime(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    pub fn exit_status(&self) -> ExitStatus {
        match self {
            Self::Usage(_) => ExitStatus::Usage,
            Self::Config(_) => ExitStatus::Config,
            Self::Runtime(_) => ExitStatus::Runtime,
            Self::Io(_) => ExitStatus::Io,
            Self::Internal(_) => ExitStatus::Internal,
        }
    }
}

/// 从 anyhow 错误推断退出码类别
pub fn app_error_from_anyhow(err: anyhow::Error) -> AppError {
    if let Some(io) = err.downcast_ref::<std::io::Error>() {
        return AppError::Io(std::io::Error::new(io.kind(), io.to_string()));
    }
    let msg = err.to_string();
    if msg.contains("配置文件未找到")
        || msg.contains("配置 version")
        || msg.contains("解析 YAML")
        || msg.contains("解析 JSON")
        || msg.contains("Pipeline 存在循环")
        || msg.contains("步骤 ID")
    {
        AppError::Config(msg)
    } else if msg.contains("未找到 Pipeline") || msg.contains("未知或未启用的模块") {
        AppError::Usage(msg)
    } else {
        AppError::Runtime(msg)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        app_error_from_anyhow(err)
    }
}
