//! Pipeline / IPC 线格式：路由字段 + 扁平 flags

use serde_json::Value;

/// 线格式调用参数（与 StepConfig / IPC Invoke 同构）
#[derive(Debug, Clone, Default)]
pub struct WireArgs {
    pub action: Option<String>,
    pub format: Option<String>,
    pub algorithm: Option<String>,
    pub flags: Value,
}

impl WireArgs {
    /// 仅 flags（depth-0 模块：copy / scrub / shade）
    pub fn flags(flags: Value) -> Self {
        Self {
            action: None,
            format: None,
            algorithm: None,
            flags,
        }
    }

    /// action + flags
    pub fn action(action: impl Into<String>, flags: Value) -> Self {
        Self {
            action: Some(action.into()),
            format: None,
            algorithm: None,
            flags,
        }
    }

    /// compression：action + format + flags
    pub fn compression(action: impl Into<String>, format: impl Into<String>, flags: Value) -> Self {
        Self {
            action: Some(action.into()),
            format: Some(format.into()),
            algorithm: None,
            flags,
        }
    }

    /// codec：action + algorithm + flags
    pub fn codec(action: impl Into<String>, algorithm: impl Into<String>, flags: Value) -> Self {
        Self {
            action: Some(action.into()),
            format: None,
            algorithm: Some(algorithm.into()),
            flags,
        }
    }
}
