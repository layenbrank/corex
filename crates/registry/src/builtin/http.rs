//! `http.send` — HTTP client (curl / fetch style).

use crate::ActionRegistry;
use crate::builtin::util::{opt_bool, opt_i64, require_map, require_str};
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use reqwest::{Client, Method, RequestBuilder};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

pub struct HttpSend;

#[async_trait]
impl Action for HttpSend {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "http.send",
            "HTTP Send",
            "发送 HTTP 请求（类 curl / fetch）：method、query、headers、token、json/form/body",
            ActionCategory::Network,
        )
        .with_params(vec![
            ParamSchema::new("url", SchemaType::Str, true).with_description("请求 URL"),
            ParamSchema::new("method", SchemaType::Str, false)
                .with_default("GET")
                .with_description("HTTP 方法：GET POST PUT PATCH DELETE HEAD OPTIONS …"),
            ParamSchema::new("params", SchemaType::Map, false)
                .with_description("URL 查询参数（同 fetch URLSearchParams / axios params）"),
            ParamSchema::new("query", SchemaType::Map, false).with_description("params 的别名"),
            ParamSchema::new("headers", SchemaType::Map, false).with_description("请求头"),
            ParamSchema::new("token", SchemaType::Str, false)
                .with_description("Bearer Token 简写，等价 Authorization: Bearer <token>"),
            ParamSchema::new("auth", SchemaType::Map, false).with_description(
                "认证：type=bearer|basic|header + token 或 username/password 或 header/value",
            ),
            ParamSchema::new("body", SchemaType::Any, false)
                .with_description("原始请求体（字符串或 bytes）"),
            ParamSchema::new("json", SchemaType::Map, false)
                .with_description("JSON 请求体，自动设置 Content-Type: application/json"),
            ParamSchema::new("form", SchemaType::Map, false)
                .with_description("表单请求体 application/x-www-form-urlencoded"),
            ParamSchema::new("timeout_ms", SchemaType::Int, false)
                .with_default(30_000)
                .with_description("超时毫秒数"),
            ParamSchema::new("follow_redirects", SchemaType::Bool, false)
                .with_default(true)
                .with_description("是否跟随重定向"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        _ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let url = require_str(map, "url")?;
        let method = parse_method(map.get("method").and_then(|v| v.as_str()).unwrap_or("GET"))?;
        let client = build_client(map)?;
        let mut builder = client.request(method, url);
        builder = apply_query(builder, map)?;
        builder = apply_headers(builder, map.get("headers"))?;
        builder = apply_auth(builder, map)?;
        builder = apply_body(builder, map)?;
        let resp = builder
            .send()
            .await
            .map_err(|e| ActionError::execution(format!("HTTP 请求失败: {e}")))?;
        response_to_value(resp).await
    }
}

fn parse_method(raw: &str) -> Result<Method, ActionError> {
    Method::from_bytes(raw.trim().to_uppercase().as_bytes()).map_err(|_| {
        ActionError::InvalidParams(format!(
            "不支持的 HTTP 方法: {raw}（示例: GET POST PUT PATCH DELETE HEAD OPTIONS）"
        ))
    })
}

fn build_client(map: &BTreeMap<String, Value>) -> Result<Client, ActionError> {
    let timeout_ms = opt_i64(map, "timeout_ms", 30_000).max(0) as u64;
    let follow = opt_bool(map, "follow_redirects", true);
    Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .redirect(if follow {
            reqwest::redirect::Policy::default()
        } else {
            reqwest::redirect::Policy::none()
        })
        .build()
        .map_err(|e| ActionError::execution(format!("创建 HTTP 客户端失败: {e}")))
}

fn query_source(map: &BTreeMap<String, Value>) -> Option<&BTreeMap<String, Value>> {
    map.get("params")
        .or_else(|| map.get("query"))
        .and_then(|v| v.as_map())
}

fn apply_query(
    mut builder: RequestBuilder,
    map: &BTreeMap<String, Value>,
) -> Result<RequestBuilder, ActionError> {
    let Some(query) = query_source(map) else {
        return Ok(builder);
    };
    for (key, value) in query {
        builder = builder.query(&[(key.as_str(), value_to_string(value))]);
    }
    Ok(builder)
}

fn apply_headers(
    mut builder: RequestBuilder,
    headers: Option<&Value>,
) -> Result<RequestBuilder, ActionError> {
    let Some(Value::Map(headers)) = headers else {
        return Ok(builder);
    };
    for (key, value) in headers {
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| ActionError::InvalidParams(format!("无效请求头名称: {key}")))?;
        let val = HeaderValue::from_str(&value_to_string(value))
            .map_err(|_| ActionError::InvalidParams(format!("无效请求头值: {key}")))?;
        builder = builder.header(name, val);
    }
    Ok(builder)
}

fn apply_auth(
    mut builder: RequestBuilder,
    map: &BTreeMap<String, Value>,
) -> Result<RequestBuilder, ActionError> {
    if let Some(token) = map.get("token").and_then(|v| v.as_str()) {
        if !token.is_empty() {
            builder = builder.bearer_auth(token);
        }
    }
    let Some(Value::Map(auth)) = map.get("auth") else {
        return Ok(builder);
    };
    match auth
        .get("type")
        .or_else(|| auth.get("scheme"))
        .and_then(|v| v.as_str())
        .unwrap_or("bearer")
        .to_ascii_lowercase()
        .as_str()
    {
        "bearer" => {
            let token = auth
                .get("token")
                .or_else(|| auth.get("value"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| ActionError::MissingParam("auth.token".into()))?;
            builder = builder.bearer_auth(token);
        }
        "basic" => {
            let username = auth
                .get("username")
                .or_else(|| auth.get("user"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| ActionError::MissingParam("auth.username".into()))?;
            let password = auth
                .get("password")
                .or_else(|| auth.get("pass"))
                .and_then(|v| v.as_str());
            builder = builder.basic_auth(username, password);
        }
        "header" | "api_key" | "apikey" => {
            let header = auth
                .get("header")
                .or_else(|| auth.get("name"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| ActionError::MissingParam("auth.header".into()))?;
            let value = auth
                .get("value")
                .or_else(|| auth.get("token"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| ActionError::MissingParam("auth.value".into()))?;
            builder = builder.header(header, value);
        }
        other => {
            return Err(ActionError::InvalidParams(format!(
                "未知 auth.type: {other}（bearer|basic|header）"
            )));
        }
    }
    Ok(builder)
}

fn apply_body(
    mut builder: RequestBuilder,
    map: &BTreeMap<String, Value>,
) -> Result<RequestBuilder, ActionError> {
    let has_json = map.get("json").is_some();
    let has_form = map.get("form").is_some();
    let has_body = map.get("body").is_some();
    if has_json as u8 + has_form as u8 + has_body as u8 > 1 {
        return Err(ActionError::InvalidParams(
            "json / form / body 只能指定其一".into(),
        ));
    }
    if let Some(json) = map.get("json") {
        builder = builder
            .json(&json.to_json())
            .header(CONTENT_TYPE, "application/json");
        return Ok(builder);
    }
    if let Some(Value::Map(form)) = map.get("form") {
        let pairs: Vec<(String, String)> = form
            .iter()
            .map(|(k, v)| (k.clone(), value_to_string(v)))
            .collect();
        builder = builder.form(&pairs);
        return Ok(builder);
    }
    if let Some(body) = map.get("body") {
        builder = match body {
            Value::Str(s) => builder.body(s.clone()),
            Value::Bytes(b) => builder.body(b.clone()),
            other => builder.body(other.to_string()),
        };
    }
    Ok(builder)
}

async fn response_to_value(resp: reqwest::Response) -> Result<Value, ActionError> {
    let status = resp.status().as_u16() as i64;
    let ok = resp.status().is_success();
    let final_url = resp.url().to_string();
    let headers_map: BTreeMap<String, Value> = resp
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
    let mut out = BTreeMap::new();
    out.insert("status".into(), Value::Int(status));
    out.insert("ok".into(), Value::Bool(ok));
    out.insert("url".into(), Value::Str(final_url));
    out.insert("headers".into(), Value::Map(headers_map));
    out.insert("body".into(), Value::Str(text));
    Ok(Value::Map(out))
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(HttpSend));
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::ExecutionContext;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn serve_once(expected_auth: Option<&str>, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let expected_auth = expected_auth.map(|s| s.to_string());
        let body = body.to_string();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            if let Some(auth) = &expected_auth {
                let req_lower = req.to_ascii_lowercase();
                let auth_lower = auth.to_ascii_lowercase();
                assert!(
                    req_lower.contains(&auth_lower),
                    "missing auth header in request: {req}"
                );
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn get_with_query_params() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(req.contains("page=2"), "query missing: {req}");
            assert!(req.contains("q=rust"), "query missing: {req}");
            let body = r#"{"ok":true}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });
        let url = format!("http://{addr}/search");
        let mut ctx = ExecutionContext::default();
        let mut m = BTreeMap::new();
        m.insert("url".into(), Value::Str(url));
        m.insert(
            "params".into(),
            Value::Map(BTreeMap::from([
                ("page".into(), Value::Int(2)),
                ("q".into(), Value::Str("rust".into())),
            ])),
        );
        let out = HttpSend
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect("http.send");
        let map = out.as_map().unwrap();
        assert_eq!(map.get("status"), Some(&Value::Int(200)));
        assert_eq!(map.get("ok"), Some(&Value::Bool(true)));
        assert!(map.get("body").unwrap().as_str().unwrap().contains("ok"));
    }

    #[tokio::test]
    async fn bearer_token_shorthand() {
        let url = serve_once(
            Some("authorization: bearer secret-token"),
            r#"{"auth":true}"#,
        )
        .await;
        let mut ctx = ExecutionContext::default();
        let mut m = BTreeMap::new();
        m.insert("url".into(), Value::Str(url));
        m.insert("token".into(), Value::Str("secret-token".into()));
        let out = HttpSend
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect("token send");
        assert_eq!(out.as_map().unwrap().get("ok"), Some(&Value::Bool(true)));
    }

    #[tokio::test]
    async fn post_json_body() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(req.contains("POST"), "expected POST: {req}");
            assert!(
                req.contains(r#""name":"corex""#),
                "json body missing: {req}"
            );
            let body = r#"{"saved":true}"#;
            let resp = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });
        let url = format!("http://{addr}/items");
        let mut ctx = ExecutionContext::default();
        let mut m = BTreeMap::new();
        m.insert("url".into(), Value::Str(url));
        m.insert("method".into(), Value::Str("POST".into()));
        m.insert(
            "json".into(),
            Value::Map(BTreeMap::from([(
                "name".into(),
                Value::Str("corex".into()),
            )])),
        );
        let out = HttpSend
            .execute(Value::Map(m), &mut ctx)
            .await
            .expect("post json");
        let map = out.as_map().unwrap();
        assert_eq!(map.get("status"), Some(&Value::Int(201)));
        assert_eq!(map.get("ok"), Some(&Value::Bool(true)));
    }

    #[test]
    fn rejects_multiple_body_sources() {
        let mut m = BTreeMap::new();
        m.insert("json".into(), Value::Map(BTreeMap::new()));
        m.insert("body".into(), Value::Str("x".into()));
        let err = apply_body(Client::new().get("http://example.com"), &m).expect_err("conflict");
        assert!(err.to_string().contains("只能指定其一"));
    }
}
