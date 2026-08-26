//! JSON-RPC-ish request / response types.

use corex_core::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum NDJSON line size (1 MiB).
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Client → daemon requests.
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
    ListShortcuts {
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
    RunShortcut {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        name: String,
        #[serde(default)]
        input: HashMap<String, Value>,
        #[serde(default)]
        path: Option<String>,
    },
    /// Invoke a single action by id.
    Invoke {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        action: String,
        #[serde(default)]
        params: Value,
    },
}

impl Request {
    pub fn id(&self) -> u64 {
        match self {
            Request::Ping { id, .. }
            | Request::Shutdown { id, .. }
            | Request::ListShortcuts { id, .. }
            | Request::ListActions { id, .. }
            | Request::RunShortcut { id, .. }
            | Request::Invoke { id, .. } => *id,
        }
    }

    pub fn auth_token(&self) -> Option<&str> {
        match self {
            Request::Ping { auth_token, .. }
            | Request::Shutdown { auth_token, .. }
            | Request::ListShortcuts { auth_token, .. }
            | Request::ListActions { auth_token, .. }
            | Request::RunShortcut { auth_token, .. }
            | Request::Invoke { auth_token, .. } => auth_token.as_deref(),
        }
    }

    /// Attach or replace the auth token on a request.
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        let t = Some(token.into());
        match &mut self {
            Request::Ping { auth_token, .. }
            | Request::Shutdown { auth_token, .. }
            | Request::ListShortcuts { auth_token, .. }
            | Request::ListActions { auth_token, .. }
            | Request::RunShortcut { auth_token, .. }
            | Request::Invoke { auth_token, .. } => *auth_token = t,
        }
        self
    }
}

/// Daemon → client responses.
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
