//! rmcp 的 `ServerHandler`：把 MCP 的 `tools/list` / `tools/call` 接到 [`State`]。

use std::sync::Arc;

use corex_core::Value;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};

use crate::exec::State;
use crate::tools;

pub struct CorexHandler {
    pub state: Arc<State>,
}

impl ServerHandler for CorexHandler {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_tool_list_changed()
            .build();
        ServerConfig::new(capabilities)
            .with_server_info(Implementation::new("corex-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "执行 corex 内置 Action 或跑一条 Directive YAML。\
                 权限与配置（strict_permissions / disabled_actions / filesystem_roots）与 corex CLI 一致。",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: tools::build(&self.state.registry),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool_name = request.name.as_ref();

        if tool_name == tools::DIRECTIVE_TOOL {
            let args = request.arguments.unwrap_or_default();
            return Ok(self.call_directive(args).await.into());
        }

        let Some(action_id) = tools::find_action_id(&self.state.registry, tool_name) else {
            return Err(McpError::invalid_params(
                format!("未知工具: {tool_name}"),
                None,
            ));
        };

        let params = match request.arguments {
            Some(map) => Value::from_json(serde_json::Value::Object(map)),
            None => Value::from_json(serde_json::Value::Object(serde_json::Map::new())),
        };

        match self.state.invoke_action(&action_id, params).await {
            Ok(value) => Ok(CallToolResult::structured(value.to_json()).into()),
            Err(e) => Ok(CallToolResult::error(vec![ContentBlock::text(err_text(&e))]).into()),
        }
    }
}

impl CorexHandler {
    async fn call_directive(&self, args: rmcp::model::JsonObject) -> CallToolResult {
        let Some(name) = args.get("name").and_then(|v| v.as_str()) else {
            return CallToolResult::error(vec![ContentBlock::text("缺少必填参数: name")]);
        };
        let path = args.get("path").and_then(|v| v.as_str());
        let input = args
            .get("input")
            .cloned()
            .map(|v| match v {
                serde_json::Value::Object(map) => map,
                _ => serde_json::Map::new(),
            })
            .unwrap_or_default();

        match self.state.run_directive(name, path, input).await {
            Ok(value) => CallToolResult::structured(value.to_json()),
            Err(e) => CallToolResult::error(vec![ContentBlock::text(err_text(&e))]),
        }
    }
}

/// 把 `anyhow` 链里第一个带类型的错误取出来，带上 `kind()` 前缀——与 daemon 的
/// `classify` 同一个做法（按类型 downcast，不按消息文本猜）。
fn err_text(err: &anyhow::Error) -> String {
    for cause in err.chain() {
        if let Some(action) = cause.downcast_ref::<corex_core::ActionError>() {
            return format!("[{}] {action}", action.kind());
        }
        if let Some(engine) = cause.downcast_ref::<corex_core::EngineError>() {
            return match engine {
                corex_core::EngineError::StepFailed { source, .. } => {
                    format!("[{}] {engine}", source.kind())
                }
                corex_core::EngineError::Action(action) => {
                    format!("[{}] {engine}", action.kind())
                }
                other => format!("[{}] {other}", other.kind()),
            };
        }
    }
    err.to_string()
}
