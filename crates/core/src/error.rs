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
    pub fn execution(msg: impl Into<String>) -> Self {
        Self::ExecutionFailed(msg.into())
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

/// Errors from shortcut loading, variable resolution, and pipeline control flow.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("快捷指令未找到: {0}")]
    ShortcutNotFound(String),

    #[error("快捷指令解析失败: {0}")]
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
