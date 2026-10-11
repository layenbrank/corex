//! rmcp `ServerHandler`：仅 dispatch 七个 meta-tool。

use std::sync::Arc;

use corex_core::Value;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler};
use serde_json::json;

use crate::errors::{self, code};
use crate::exec::State;
use crate::tools;

pub struct CorexHandler {
    pub state: Arc<State>,
}

impl ServerHandler for CorexHandler {
    fn get_info(&self) -> ServerConfig {
        // 不声明 listChanged：本轮不做真推送，避免假 capability。
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        ServerConfig::new(capabilities)
            .with_server_info(Implementation::new("corex-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "corex Agent 工具面（渐进披露）。发现：corex_actions→corex_action 或 \
                 corex_directives→corex_directive→corex_validate；执行：corex_invoke / corex_run。\
                 调用同步阻塞直至完成。权限与配置（strict_permissions / disabled_actions / \
                 filesystem_roots / [mcp] allow_* / is_deny_default）与 corex CLI 一致。\
                 stdio 信任 OS 用户边界；HTTP 另需 COREX_TOKEN。",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: tools::meta_tools(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool_name = request.name.as_ref();
        if !tools::is_meta_tool(tool_name) {
            return Err(McpError::invalid_params(
                format!("未知工具: {tool_name}"),
                Some(json!({ "code": code::UNKNOWN_TOOL })),
            ));
        }
        let args = request.arguments.unwrap_or_default();
        Ok(self.dispatch(tool_name, args).await.into())
    }
}

impl CorexHandler {
    async fn dispatch(&self, tool: &str, args: rmcp::model::JsonObject) -> CallToolResult {
        match tool {
            tools::ACTIONS => self.call_actions(args),
            tools::ACTION => self.call_action(args),
            tools::INVOKE => self.call_invoke(args).await,
            tools::DIRECTIVES => self.call_directives(args),
            tools::DIRECTIVE => self.call_directive(args),
            tools::VALIDATE => self.call_validate(args),
            tools::RUN => self.call_run(args).await,
            _ => errors::tool_failure(code::UNKNOWN_TOOL, format!("未知工具: {tool}")),
        }
    }

    fn call_actions(&self, args: rmcp::model::JsonObject) -> CallToolResult {
        let bucket = args.get("bucket").and_then(|v| v.as_str());
        let ids = parse_string_array(args.get("ids"));
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        match self.state.fetch_actions(bucket, ids.as_deref(), limit) {
            Ok(value) => CallToolResult::structured(value.to_json()),
            Err(e) => errors::from_anyhow(&e.into()),
        }
    }

    fn call_action(&self, args: rmcp::model::JsonObject) -> CallToolResult {
        let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
            return errors::tool_failure(code::INVALID_PARAMS, "缺少必填参数: id");
        };
        match self.state.find_action(id) {
            Ok(value) => CallToolResult::structured(value.to_json()),
            Err(e) => errors::from_anyhow(&e.into()),
        }
    }

    async fn call_invoke(&self, args: rmcp::model::JsonObject) -> CallToolResult {
        let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
            return errors::tool_failure(code::INVALID_PARAMS, "缺少必填参数: id");
        };
        let params = match args.get("params") {
            Some(serde_json::Value::Object(map)) => {
                Value::from_json(serde_json::Value::Object(map.clone()))
            }
            Some(_) => {
                return errors::tool_failure(code::INVALID_PARAMS, "params 必须是对象");
            }
            None => Value::from_json(serde_json::Value::Object(serde_json::Map::new())),
        };
        match self.state.invoke_action(id, params, tools::INVOKE).await {
            Ok(value) => CallToolResult::structured(value.to_json()),
            Err(e) => errors::from_anyhow(&e),
        }
    }

    fn call_directives(&self, args: rmcp::model::JsonObject) -> CallToolResult {
        let names = parse_string_array(args.get("names"));
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        match self.state.fetch_directives(names.as_deref(), limit) {
            Ok(value) => CallToolResult::structured(value.to_json()),
            Err(e) => errors::from_anyhow(&e),
        }
    }

    fn call_directive(&self, args: rmcp::model::JsonObject) -> CallToolResult {
        let Some(name) = args.get("name").and_then(|v| v.as_str()) else {
            return errors::tool_failure(code::INVALID_PARAMS, "缺少必填参数: name");
        };
        match self.state.fetch_directive(name) {
            Ok(value) => CallToolResult::structured(value.to_json()),
            Err(e) => errors::from_anyhow(&e),
        }
    }

    fn call_validate(&self, args: rmcp::model::JsonObject) -> CallToolResult {
        let Some(definition) = args.get("definition").cloned() else {
            return errors::tool_failure(code::INVALID_PARAMS, "缺少必填参数: definition");
        };
        let strict = args
            .get("strict")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        match self.state.validate_directive(definition, strict) {
            Ok(value) => CallToolResult::structured(value.to_json()),
            Err(e) => errors::from_anyhow(&e),
        }
    }

    async fn call_run(&self, args: rmcp::model::JsonObject) -> CallToolResult {
        let name = args.get("name").and_then(|v| v.as_str());
        let path = args.get("path").and_then(|v| v.as_str());
        match (name, path) {
            (None, None) => {
                return errors::tool_failure(
                    code::INVALID_PARAMS,
                    "corex_run 需要 name 或 path 之一",
                );
            }
            (Some(_), Some(_)) => {
                return errors::tool_failure(
                    code::INVALID_PARAMS,
                    "corex_run 的 name 与 path 互斥，只能给一个",
                );
            }
            _ => {}
        }
        let input = args
            .get("input")
            .cloned()
            .map(|v| match v {
                serde_json::Value::Object(map) => map,
                _ => serde_json::Map::new(),
            })
            .unwrap_or_default();
        match self
            .state
            .run_directive(name, path, input, tools::RUN)
            .await
        {
            Ok(value) => CallToolResult::structured(value.to_json()),
            Err(e) => errors::from_anyhow(&e),
        }
    }
}

fn parse_string_array(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    value.and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect()
    })
}
