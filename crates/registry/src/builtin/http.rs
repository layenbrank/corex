//! `http.request` — HTTP client via reqwest.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;

pub struct HttpRequest;

#[async_trait]
impl Action for HttpRequest {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "http.request",
            "HTTP Request",
            "发送 HTTP 请求并返回状态码与正文",
            ActionCategory::Network,
        )
        .with_params(vec![
            ParamSchema::new("url", SchemaType::Str, true),
            ParamSchema::new("method", SchemaType::Str, false).with_default("GET"),
            ParamSchema::new("headers", SchemaType::Map, false),
            ParamSchema::new("body", SchemaType::Any, false),
            ParamSchema::new("json", SchemaType::Map, false),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".to_string()))?;

        let url = map
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("url".to_string()))?;
        let method = map
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();

        let client = reqwest::Client::new();
        let mut builder = match method.as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            "HEAD" => client.head(url),
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的 HTTP 方法: {other}"
                )));
            }
        };

        if let Some(Value::Map(headers)) = map.get("headers") {
            for (k, v) in headers {
                if let Some(s) = v.as_str() {
                    builder = builder.header(k, s);
                }
            }
        }

        if let Some(json) = map.get("json") {
            builder = builder
                .json(&json.to_json())
                .header(reqwest::header::CONTENT_TYPE, "application/json");
        } else if let Some(body) = map.get("body") {
            match body {
                Value::Str(s) => builder = builder.body(s.clone()),
                Value::Bytes(b) => builder = builder.body(b.clone()),
                other => builder = builder.body(other.to_string()),
            }
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| ActionError::execution(format!("HTTP 请求失败: {e}")))?;

        let status = resp.status().as_u16() as i64;
        let headers_map: std::collections::BTreeMap<String, Value> = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    Value::Str(v.to_str().unwrap_or("").to_string()),
                )
            })
            .collect();
        let text = resp
            .text()
            .await
            .map_err(|e| ActionError::execution(format!("读取响应失败: {e}")))?;

        let mut out = std::collections::BTreeMap::new();
        out.insert("status".into(), Value::Int(status));
        out.insert("headers".into(), Value::Map(headers_map));
        out.insert("body".into(), Value::Str(text));
        Ok(Value::Map(out))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(HttpRequest));
}
