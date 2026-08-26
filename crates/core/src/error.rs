//! Error types for actions and the pipeline engine.

use thiserror::Error;

/// Errors originating from a single action execution / validation.
#[derive(Debug, Error)]
pub enum ActionError {
    #[error("缺少必需参数: {0}")]
    MissingParam(String),

    #[error("参数无效: {0}")]
    InvalidParams(String),

    #[error("动作执行失败: {0}")]
    ExecutionFailed(String),

    #[error("动作未找到: {0}")]
    NotFound(String),

    #[error("动作已禁用: {0}")]
    Disabled(String),

    #[error("权限不足: {0}")]
    PermissionDenied(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl ActionError {
    pub fn execution(msg: impl AsRef<str>) -> Self {
        Self::ExecutionFailed(msg.as_ref().to_string())
    }

    pub fn ui(code: &str, msg: impl AsRef<str>) -> Self {
        Self::ExecutionFailed(format!("[{code}] {}", msg.as_ref()))
    }

    /// UI error with a redacted selector hint for audit (`[selector_hint=...]`).
    pub fn ui_with_hint(code: &str, hint: &str, msg: impl AsRef<str>) -> Self {
        Self::ExecutionFailed(format!("[{code}][selector_hint={hint}] {}", msg.as_ref()))
    }

    /// Parse UI error code prefix `[ui_*]` if present.
    pub fn ui_code(&self) -> Option<&str> {
        match self {
            Self::ExecutionFailed(s) | Self::Other(s) | Self::Timeout(s) => {
                s.strip_prefix('[').and_then(|rest| {
                    let end = rest.find(']')?;
                    let code = &rest[..end];
                    if code.starts_with("ui_") { Some(code) } else { None }
                })
            }
            _ => None,
        }
    }

    pub fn other(msg: impl AsRef<str>) -> Self {
        Self::Other(msg.as_ref().to_string())
    }
}

/// Errors from Directive loading, variable resolution, and pipeline control flow.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("指令未找到: {0}")]
    DirectiveNotFound(String),

    #[error("指令解析失败: {0}")]
    ParseError(String),

    #[error("变量未定义: {0}")]
    UndefinedVariable(String),

    #[error("变量解析失败: {0}")]
    ResolveError(String),

    #[error("条件求值失败: {0}")]
    ConditionError(String),

    #[error("步骤执行失败 [{step}]: {source}")]
    StepFailed {
        step: String,
        #[source]
        source: ActionError,
    },

    #[error("动作未注册: {0}")]
    ActionNotRegistered(String),

    #[error("控制流错误: {0}")]
    ControlFlow(String),

    #[error("配置错误: {0}")]
    Config(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    Action(#[from] ActionError),
}

impl EngineError {
    pub fn parse(msg: impl Into<String>) -> Self {
        Self::ParseError(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}
