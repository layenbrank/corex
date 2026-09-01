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

    /// Parse UI error code `[ui_*]` from bracket tags in the message.
    pub fn ui_code(&self) -> Option<&str> {
        self.tagged_message()
            .and_then(|msg| bracket_segments(msg).find(|s| s.starts_with("ui_")))
    }

    /// Redacted selector hint from `[selector_hint=...]` in the message.
    pub fn selector_hint(&self) -> Option<&str> {
        self.tagged_message().and_then(|msg| {
            bracket_segments(msg).find_map(|s| {
                s.strip_prefix("selector_hint=")
                    .filter(|hint| !hint.is_empty())
            })
        })
    }

    pub fn other(msg: impl AsRef<str>) -> Self {
        Self::Other(msg.as_ref().to_string())
    }

    /// Stable machine-readable error kind for audit / history.
    pub fn kind(&self) -> String {
        match self {
            Self::PermissionDenied(_) => "permission_denied".into(),
            Self::Timeout(_) => "timeout".into(),
            Self::MissingParam(_) | Self::InvalidParams(_) => "invalid_params".into(),
            Self::NotFound(_) => "not_found".into(),
            Self::Disabled(_) => "disabled".into(),
            Self::Io(_) => "io".into(),
            Self::ExecutionFailed(_) | Self::Other(_) => self
                .ui_code()
                .map(str::to_string)
                .unwrap_or_else(|| "execution".into()),
        }
    }

    pub fn is_permission_denied(&self) -> bool {
        matches!(self, Self::PermissionDenied(_))
    }

    fn tagged_message(&self) -> Option<&str> {
        match self {
            Self::ExecutionFailed(s) | Self::Other(s) | Self::Timeout(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// Bracket tags in messages such as `[ui_login_pending][selector_hint=...]`.
fn bracket_segments(msg: &str) -> impl Iterator<Item = &str> {
    msg.split(']').filter_map(|part| {
        let start = part.rfind('[')?;
        Some(&part[start + 1..])
    })
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

    /// Stable machine-readable error kind for audit / history.
    pub fn kind(&self) -> String {
        match self {
            Self::ActionNotRegistered(_) => "not_registered".into(),
            Self::StepFailed { source, .. } => source.kind(),
            Self::Action(err) => err.kind(),
            Self::DirectiveNotFound(_) => "not_found".into(),
            Self::ParseError(_) => "parse".into(),
            Self::UndefinedVariable(_) | Self::ResolveError(_) => "resolve".into(),
            Self::ConditionError(_) | Self::ControlFlow(_) => "control_flow".into(),
            Self::Config(_) => "config".into(),
            Self::Io(_) => "io".into(),
            Self::Other(_) => "execution".into(),
        }
    }

    pub fn is_permission_denied(&self) -> bool {
        match self {
            Self::StepFailed { source, .. } => source.is_permission_denied(),
            Self::Action(err) => err.is_permission_denied(),
            _ => false,
        }
    }

    /// Action-level source when present (`StepFailed` / `Action`).
    pub fn action_source(&self) -> Option<&ActionError> {
        match self {
            Self::StepFailed { source, .. } => Some(source),
            Self::Action(err) => Some(err),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ui_tags_from_message() {
        let err = ActionError::ui_with_hint("ui_login_pending", "name=进入微信", "超时");
        assert_eq!(err.ui_code(), Some("ui_login_pending"));
        assert_eq!(err.selector_hint(), Some("name=进入微信"));
        assert_eq!(err.kind(), "ui_login_pending");
    }

    #[test]
    fn engine_kind_delegates_to_action() {
        let err = EngineError::StepFailed {
            step: "s1".into(),
            source: ActionError::PermissionDenied("x".into()),
        };
        assert!(err.is_permission_denied());
        assert_eq!(err.kind(), "permission_denied");
    }
}
