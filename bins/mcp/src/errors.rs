//! MCP 工具失败契约：`structuredContent = { code, message, retryable }`；
//! `content` 仅承载同一 `message`（无双格式分支）。

use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::json;

/// 稳定错误码（`ActionError` / `EngineError` 的 `kind()` 子集 + 策略码）。
pub mod code {
    pub const UNKNOWN_TOOL: &str = "unknown_tool";
    pub const INVALID_PARAMS: &str = "invalid_params";
    pub const POLICY_DENIED: &str = "policy_denied";
    pub const NOT_FOUND: &str = "not_found";
    pub const PERMISSION_DENIED: &str = "permission_denied";
    pub const TIMEOUT: &str = "timeout";
    pub const DISABLED: &str = "disabled";
    pub const NOT_REGISTERED: &str = "not_registered";
    pub const PARSE: &str = "parse";
    pub const RESOLVE: &str = "resolve";
    pub const CONTROL_FLOW: &str = "control_flow";
    pub const CONFIG: &str = "config";
    pub const USAGE: &str = "usage";
    pub const CONFLICT: &str = "conflict";
    pub const IO: &str = "io";
    pub const EXECUTION: &str = "execution";
}

/// 构造工具级失败：content 与 structured.message 同文。
pub fn tool_failure(code: &str, message: impl Into<String>) -> CallToolResult {
    let message = message.into();
    let retryable = is_retryable(code);
    let structured = json!({
        "code": code,
        "message": message,
        "retryable": retryable,
    });
    // CallToolResult 是 non_exhaustive：先造 error 再挂 structured。
    let mut result = CallToolResult::error(vec![ContentBlock::text(message)]);
    result.structured_content = Some(structured);
    result
}

pub fn from_anyhow(err: &anyhow::Error) -> CallToolResult {
    let (code, message) = classify(err);
    tool_failure(code, message)
}

fn classify(err: &anyhow::Error) -> (&'static str, String) {
    for cause in err.chain() {
        if let Some(action) = cause.downcast_ref::<corex_core::ActionError>() {
            let message = action.to_string();
            if matches!(action, corex_core::ActionError::PermissionDenied(_))
                && message.contains("MCP 策略")
            {
                return (code::POLICY_DENIED, message);
            }
            return (stabilize_action(action.kind()), message);
        }
        if let Some(engine) = cause.downcast_ref::<corex_core::EngineError>() {
            let kind = match engine {
                corex_core::EngineError::StepFailed { source, .. } => source.kind(),
                corex_core::EngineError::Action(action) => action.kind(),
                other => other.kind(),
            };
            return (stabilize_engine(&kind), engine.to_string());
        }
        if let Some(store) = cause.downcast_ref::<corex_store::StoreError>() {
            return (stabilize_engine(store.kind()), store.to_string());
        }
    }
    (code::EXECUTION, err.to_string())
}

fn stabilize_action(kind: String) -> &'static str {
    match kind.as_str() {
        "permission_denied" => code::PERMISSION_DENIED,
        "timeout" => code::TIMEOUT,
        "invalid_params" => code::INVALID_PARAMS,
        "not_found" => code::NOT_FOUND,
        "disabled" => code::DISABLED,
        "io" => code::IO,
        // ui_* 等细码仍归 execution；retryable 见 is_retryable
        other if other.starts_with("ui_") => code::EXECUTION,
        _ => code::EXECUTION,
    }
}

fn stabilize_engine(kind: &str) -> &'static str {
    match kind {
        "not_registered" => code::NOT_REGISTERED,
        "not_found" => code::NOT_FOUND,
        "parse" => code::PARSE,
        "resolve" => code::RESOLVE,
        "control_flow" => code::CONTROL_FLOW,
        "config" => code::CONFIG,
        "usage" => code::USAGE,
        "conflict" => code::CONFLICT,
        "io" => code::IO,
        "permission_denied" => code::PERMISSION_DENIED,
        "timeout" => code::TIMEOUT,
        "invalid_params" => code::INVALID_PARAMS,
        "disabled" => code::DISABLED,
        _ => code::EXECUTION,
    }
}

/// 可重试：超时 / 冲突 / IO；策略与参数类不可重试。
pub fn is_retryable(code_name: &str) -> bool {
    matches!(
        code_name,
        code::TIMEOUT | code::CONFLICT | code::IO | code::EXECUTION
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_and_unknown_are_not_retryable() {
        assert!(!is_retryable(code::POLICY_DENIED));
        assert!(!is_retryable(code::UNKNOWN_TOOL));
        assert!(!is_retryable(code::INVALID_PARAMS));
        assert!(is_retryable(code::TIMEOUT));
    }

    #[test]
    fn failure_content_matches_structured_message() {
        let result = tool_failure(code::POLICY_DENIED, "不允许");
        assert_eq!(result.is_error, Some(true));
        let structured = result.structured_content.expect("structured");
        assert_eq!(structured["code"], "policy_denied");
        assert_eq!(structured["message"], "不允许");
        assert_eq!(structured["retryable"], false);
        match &result.content[0] {
            ContentBlock::Text(text) => assert_eq!(text.text, "不允许"),
            other => panic!("expected text, got {other:?}"),
        }
    }
}
