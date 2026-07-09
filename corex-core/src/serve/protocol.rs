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
    pub ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn success(id: u64, path: Option<String>, ms: u64) -> Self {
        Self {
            id,
            ok: true,
            path,
            ms,
            error: None,
        }
    }

    pub fn failure(id: u64, error: impl Into<String>, ms: u64) -> Self {
        Self {
            id,
            ok: false,
            path: None,
            ms,
            error: Some(error.into()),
        }
    }
}

/// 解析单行 JSON 请求
pub fn parse_request(line: &str) -> anyhow::Result<Request> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        anyhow::bail!("空请求");
    }

    if let Ok(req) = serde_json::from_str::<Request>(trimmed) {
        return Ok(req);
    }

    // 兼容简写：{"id":1,"module":"screenshot","args":{...}}
    #[derive(Deserialize)]
    struct LegacyInvoke {
        id: u64,
        module: String,
        args: Value,
    }

    if let Ok(legacy) = serde_json::from_str::<LegacyInvoke>(trimmed) {
        return Ok(Request::Invoke {
            id: legacy.id,
            module: legacy.module,
            args: legacy.args,
        });
    }

    #[derive(Deserialize)]
    struct LegacyShutdown {
        cmd: String,
    }

    if let Ok(shutdown) = serde_json::from_str::<LegacyShutdown>(trimmed) {
        if shutdown.cmd == "shutdown" {
            return Ok(Request::Shutdown);
        }
    }

    anyhow::bail!("无法解析请求: {trimmed}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_typed_invoke() {
        let line = r#"{"type":"invoke","id":1,"module":"screenshot","args":{"to":"/tmp"}}"#;
        let req = parse_request(line).unwrap();
        match req {
            Request::Invoke { id, module, .. } => {
                assert_eq!(id, 1);
                assert_eq!(module, "screenshot");
            }
            _ => panic!("expected invoke"),
        }
    }

    #[test]
    fn parse_legacy_invoke() {
        let line = r#"{"id":2,"module":"copy","args":{"from":"a","to":"b"}}"#;
        let req = parse_request(line).unwrap();
        match req {
            Request::Invoke { id, module, .. } => {
                assert_eq!(id, 2);
                assert_eq!(module, "copy");
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
    fn parse_legacy_shutdown() {
        let line = r#"{"cmd":"shutdown"}"#;
        assert!(matches!(parse_request(line).unwrap(), Request::Shutdown));
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
