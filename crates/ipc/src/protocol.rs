//! 类 JSON-RPC 的请求 / 响应类型。

use crate::progress::ProgressEvent;
use corex_core::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// NDJSON 单行最大长度（1 MiB）。
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// 客户端 → daemon 的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Ping {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
    },
    Shutdown {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
    },
    ListDirectives {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        #[serde(default)]
        dir: Option<String>,
    },
    ListActions {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
    },
    RunDirective {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        name: String,
        #[serde(default)]
        input: HashMap<String, Value>,
        #[serde(default)]
        path: Option<String>,
        /// 为真时 daemon 会在这条请求期间推 `event` 帧（进度）。
        ///
        /// 默认关：不置位的客户端拿到的线与旧版完全一致，而多推的帧会被
        /// 只读一行的一问一答客户端误当成终帧。
        #[serde(default)]
        stream: bool,
    },
    /// 按 id 调用单个动作。
    Invoke {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        action: String,
        #[serde(default)]
        params: Value,
        /// 同 [`Request::RunDirective`] 的 `stream`。
        #[serde(default)]
        stream: bool,
    },
}

impl Request {
    pub fn id(&self) -> u64 {
        match self {
            Request::Ping { id, .. }
            | Request::Shutdown { id, .. }
            | Request::ListDirectives { id, .. }
            | Request::ListActions { id, .. }
            | Request::RunDirective { id, .. }
            | Request::Invoke { id, .. } => *id,
        }
    }

    /// 本请求是否要求 daemon 推中间帧。
    ///
    /// 只有 `run_directive` 与 `invoke` 能开；其余变体没有这个字段，永远是 `false`。
    pub fn wants_stream(&self) -> bool {
        match self {
            Request::RunDirective { stream, .. } | Request::Invoke { stream, .. } => *stream,
            _ => false,
        }
    }

    pub fn auth_token(&self) -> Option<&str> {
        match self {
            Request::Ping { auth_token, .. }
            | Request::Shutdown { auth_token, .. }
            | Request::ListDirectives { auth_token, .. }
            | Request::ListActions { auth_token, .. }
            | Request::RunDirective { auth_token, .. }
            | Request::Invoke { auth_token, .. } => auth_token.as_deref(),
        }
    }

    /// 给请求挂上或替换鉴权 token。
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        let t = Some(token.into());
        match &mut self {
            Request::Ping { auth_token, .. }
            | Request::Shutdown { auth_token, .. }
            | Request::ListDirectives { auth_token, .. }
            | Request::ListActions { auth_token, .. }
            | Request::RunDirective { auth_token, .. }
            | Request::Invoke { auth_token, .. } => *auth_token = t,
        }
        self
    }
}

/// daemon → 客户端的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong {
        id: u64,
    },
    Ok {
        id: u64,
        #[serde(default)]
        data: Value,
    },
    Error {
        id: u64,
        error: RpcError,
    },
    /// 同一 `id` 请求的**中间帧**：只在请求置了 `stream` 时出现。
    ///
    /// 一条请求可以有零个或多个 `event`，它们一定排在终帧（`ok` / `error`）
    /// 之前；收到它的一方不该把它当成一次回答的结束。
    Event {
        id: u64,
        progress: ProgressEvent,
    },
    Bye {
        id: u64,
    },
}

impl Response {
    pub fn ok(id: u64, data: impl Into<Value>) -> Self {
        Self::Ok {
            id,
            data: data.into(),
        }
    }

    pub fn error(id: u64, error: RpcError) -> Self {
        Self::Error { id, error }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(404, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(500, msg)
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::new(400, msg)
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(401, msg)
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(403, msg)
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for RpcError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_line_bytes_is_one_mib() {
        assert_eq!(MAX_LINE_BYTES, 1024 * 1024);
    }

    #[test]
    fn request_auth_token_roundtrip() {
        let req = Request::Ping {
            id: 42,
            auth_token: Some("secret-token".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("auth_token"));
        assert!(json.contains("secret-token"));
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id(), 42);
        assert_eq!(back.auth_token(), Some("secret-token"));
    }

    #[test]
    fn request_auth_token_omitted_deserializes_none() {
        let json = r#"{"type":"ping","id":1}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.auth_token(), None);
    }

    #[test]
    fn with_auth_token_sets_field() {
        let req = Request::Invoke {
            id: 7,
            auth_token: None,
            action: "template.render".into(),
            params: Value::Null,
            stream: false,
        }
        .with_auth_token("tok");
        assert_eq!(req.auth_token(), Some("tok"));
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["auth_token"], "tok");
    }

    /// 旧客户端的线上形状里没有 `stream`：它必须默认成关，否则升级 daemon 就会
    /// 给只读一行的老客户端塞进一个它认不出的帧。
    #[test]
    fn a_request_without_stream_stays_one_shot() {
        let json = r#"{"type":"invoke","id":1,"action":"template.render"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(!req.wants_stream());

        let json = r#"{"type":"run_directive","id":2,"name":"hello"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(!req.wants_stream());
    }

    #[test]
    fn only_the_two_long_running_requests_can_stream() {
        let json = r#"{"type":"run_directive","id":2,"name":"hello","stream":true}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(req.wants_stream());

        // `ping` 之类没有这个字段，永远不上报。
        let json = r#"{"type":"ping","id":3}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(!req.wants_stream());
    }
}
