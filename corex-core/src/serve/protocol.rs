use serde::{Deserialize, Serialize};
use serde_json::Value;

/// IPC 请求
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// 执行业务模块
    Invoke {
        id: u64,
        module: String,
        #[serde(default)]
        action: Option<String>,
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        algorithm: Option<String>,
        #[serde(default)]
        args: Value,
    },
    /// 关闭 Daemon
    Shutdown,
}

/// IPC 响应
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    pub ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn success(id: u64, path: Option<String>, data: Option<Value>, ms: u64) -> Self {
        Self {
            id,
            ok: true,
            path,
            data,
            ms,
            error: None,
        }
    }

    pub fn failure(id: u64, error: impl Into<String>, ms: u64) -> Self {
        Self {
            id,
            ok: false,
            path: None,
            data: None,
            ms,
            error: Some(error.into()),
        }
    }
}

/// 解析单行 JSON 请求（仅支持 typed 格式）
pub fn parse_request(line: &str) -> anyhow::Result<Request> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        anyhow::bail!("空请求");
    }

    serde_json::from_str::<Request>(trimmed).map_err(|err| anyhow::anyhow!("无法解析请求: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_typed_invoke() {
        let line = r#"{"type":"invoke","id":1,"module":"screenshot","action":"capture","args":{"to":"/tmp"}}"#;
        let req = parse_request(line).unwrap();
        match req {
            Request::Invoke {
                id,
                module,
                action,
                ..
            } => {
                assert_eq!(id, 1);
                assert_eq!(module, "screenshot");
                assert_eq!(action.as_deref(), Some("capture"));
            }
            _ => panic!("expected invoke"),
        }
    }

    #[test]
    fn parse_typed_shutdown() {
        let line = r#"{"type":"shutdown"}"#;
        assert!(matches!(parse_request(line).unwrap(), Request::Shutdown));
    }

    #[test]
    fn parse_legacy_invoke_fails() {
        let line = r#"{"id":2,"module":"copy","args":{"from":"a","to":"b"}}"#;
        assert!(parse_request(line).is_err());
    }

    #[test]
    fn parse_legacy_shutdown_fails() {
        let line = r#"{"cmd":"shutdown"}"#;
        assert!(parse_request(line).is_err());
    }

    #[test]
    fn parse_empty_fails() {
        assert!(parse_request("").is_err());
        assert!(parse_request("   ").is_err());
    }

    #[test]
    fn parse_invalid_fails() {
        assert!(parse_request("not json").is_err());
    }
}
