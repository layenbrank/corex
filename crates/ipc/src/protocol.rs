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
    /// 列出指令库里的指令。
    ///
    /// v12 的 `dir` 参数没有了：指令的真相从「<数据目录>/directives 下的 YAML 文件」换成了
    /// `<数据目录>/directives.db`，分组由每条指令自己的 `folder` 字段表达，不再靠子目录。
    ListDirectives {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
    },
    /// 读一条指令：规范化 YAML 与解析结果一起给，宿主不必自己拼 YAML。
    ReadDirective {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        name: String,
    },
    /// 写一条指令：`definition` 是宿主编辑器里的结构化模型，由 daemon 校验后落库。
    ///
    /// - `original_name` 与 `name` 不同 = **改名**：daemon 在一个事务里删旧键、写新键，
    ///   v12 那套「拿新名字另存一份文件、旧文件留在目录里」的操作就此消失。
    /// - 宿主**不要**自己序列化 YAML——写出去的格式只有引擎一份。
    SaveDirective {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        name: String,
        definition: Value,
        #[serde(default)]
        original_name: Option<String>,
    },
    /// 删掉一条指令。
    DeleteDirective {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        name: String,
    },
    /// 从磁盘导入指令（YAML 文件或目录）。
    ///
    /// 这是 v13 里 YAML 的入口：`path` 是文件就导入一条，是目录就递归导入，相对子目录成为
    /// 分组。写入前先过与 `run` 同样的两道门，写进去的都是跑得起来的。
    ImportDirectives {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        /// 文件或目录路径。
        path: String,
        /// 目录导入时的兜底分组（子目录优先）。
        #[serde(default)]
        folder: Option<String>,
        /// 同名时覆盖；默认跳过并在报告里记一笔。
        #[serde(default)]
        is_overwrite: bool,
        /// 只解析校验，不写库。
        #[serde(default)]
        is_dry_run: bool,
    },
    /// 列最近的执行记录：卡片的「上次跑成什么样」只该有一个来源。
    ///
    /// 历史是引擎在跑完的当口自己写的（`[history]` 配置，默认开）；这里开的是**只读**
    /// 出口——宿主再存一份「上次运行时间」就会与这份账本各说各话。
    ListRuns {
        #[serde(default)]
        id: u64,
        #[serde(default)]
        auth_token: Option<String>,
        /// 只看这条指令；不给就是全部。
        #[serde(default)]
        name: Option<String>,
        /// 最多几条；不给用 daemon 的默认条数。
        #[serde(default)]
        limit: Option<usize>,
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
            | Request::ReadDirective { id, .. }
            | Request::SaveDirective { id, .. }
            | Request::DeleteDirective { id, .. }
            | Request::ImportDirectives { id, .. }
            | Request::ListRuns { id, .. }
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

    /// 本请求会不会真的执行东西（`run_directive` / `invoke`）。
    ///
    /// daemon 只给这类请求排 `max_jobs` 的队、只给它们起心跳：控制类请求（探活、状态、
    /// 列目录）答得越快越好。
    pub fn is_execution(&self) -> bool {
        matches!(self, Request::RunDirective { .. } | Request::Invoke { .. })
    }

    pub fn auth_token(&self) -> Option<&str> {
        match self {
            Request::Ping { auth_token, .. }
            | Request::Shutdown { auth_token, .. }
            | Request::ListDirectives { auth_token, .. }
            | Request::ReadDirective { auth_token, .. }
            | Request::SaveDirective { auth_token, .. }
            | Request::DeleteDirective { auth_token, .. }
            | Request::ImportDirectives { auth_token, .. }
            | Request::ListRuns { auth_token, .. }
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
            | Request::ReadDirective { auth_token, .. }
            | Request::SaveDirective { auth_token, .. }
            | Request::DeleteDirective { auth_token, .. }
            | Request::ImportDirectives { auth_token, .. }
            | Request::ListRuns { auth_token, .. }
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

    /// 名字撞车：目标已存在，或者并发下被别人抢先改了。
    ///
    /// 与 400 分开是因为调用方能做的事不一样：400 要改自己发的内容，409 要换个名字或先删旧的。
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(409, msg)
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

        // 编辑类请求同样是控制请求：一条都不能推中间帧。
        let json = r#"{"type":"save_directive","id":4,"name":"hello","definition":{}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(!req.wants_stream());
    }

    /// 编辑类请求要能带鉴权 token，也要能顺手替换——三个 match 少写一个就会静默漏掉。
    #[test]
    fn the_editing_requests_carry_auth() {
        let json = r#"{"type":"read_directive","id":9,"name":"build","auth_token":"tok"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.id(), 9);
        assert_eq!(req.auth_token(), Some("tok"));
        assert_eq!(
            req.with_auth_token("other").auth_token(),
            Some("other"),
            "with_auth_token 必须覆盖已有 token"
        );
    }

    /// `save_directive` 的定义是结构化模型：原样带着走，daemon 才好在落盘前校验它。
    #[test]
    fn save_directive_keeps_the_definition() {
        let json = r#"{"type":"save_directive","id":5,"name":"build","definition":{"name":"build","steps":[{"id":"a","action":"template.render"}]}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        let Request::SaveDirective {
            name, definition, ..
        } = &req
        else {
            panic!("判成了别的请求: {req:?}");
        };
        assert_eq!(name, "build");
        assert_eq!(
            definition.to_json()["steps"][0]["action"],
            serde_json::json!("template.render")
        );

        // 原样回写：宿主发的 JSON 与 daemon 解析出来的形状一致。
        let back: Request = serde_json::from_value(serde_json::to_value(&req).unwrap()).unwrap();
        assert_eq!(back.id(), 5);
        assert!(matches!(back, Request::SaveDirective { .. }));
    }

    /// 读历史是控制请求：要能带 token，但不排队、也不推中间帧。
    ///
    /// 过滤与条数都可选——只想知道「最近跑了什么」的客户端不该被迫多写字段。
    #[test]
    fn list_runs_is_a_read_only_control_request() {
        let json = r#"{"type":"list_runs","id":6,"auth_token":"tok"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.id(), 6);
        assert_eq!(req.auth_token(), Some("tok"));
        assert!(!req.is_execution());
        assert!(!req.wants_stream());

        let json = r#"{"type":"list_runs","id":7,"name":"build","limit":20}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        let Request::ListRuns { name, limit, .. } = req.with_auth_token("tok") else {
            panic!("判成了别的请求");
        };
        assert_eq!(name.as_deref(), Some("build"));
        assert_eq!(limit, Some(20));
    }
}
