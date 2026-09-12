//! `http.send` —— HTTP 客户端（curl / fetch 风格）。

use crate::ActionRegistry;
use crate::builtin::util::{
    MAX_RANGE, as_bytes, confine_path, opt_bool, opt_i64, opt_str, range_params, read_range,
    require_map, require_str,
};
use async_trait::async_trait;
use corex_core::{
    Action, ActionError, ActionMeta, Bucket, ExecutionContext, ParamSchema, PermissionSet,
    SchemaType, Unit, Value,
};
use reqwest::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use reqwest::multipart::{Form, Part};
use reqwest::{Client, Method, RequestBuilder};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub struct HttpSend;

#[async_trait]
impl Action for HttpSend {
    fn permissions(&self) -> PermissionSet {
        PermissionSet::NETWORK
    }

    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "http.send",
            "HTTP 请求",
            "发送 HTTP 请求（类 curl / fetch）：method、query、headers、token、json/form/body",
            Bucket::Network,
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
            ParamSchema::new("token", SchemaType::Secret, false)
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
            ParamSchema::new("multipart", SchemaType::Map, false).with_description(
                "multipart/form-data：字段名 → 标量按文本字段发，`{path, offset?, length?, filename?, content_type?}` 按文件部件发",
            ),
            ParamSchema::new("timeout_ms", SchemaType::Int, false)
                .with_default(30_000)
                .with_description("超时毫秒数"),
            ParamSchema::new("follow_redirects", SchemaType::Bool, false)
                .with_default(true)
                .with_description("是否跟随重定向"),
            ParamSchema::new("response", SchemaType::Str, false)
                .with_default("text")
                .with_description("text 返回字符串 | binary 返回 Bytes（下载图片/压缩包等）"),
            ParamSchema::new("encoding", SchemaType::Str, false).with_description(
                "响应字符集，如 gbk / gb18030 / big5；不填则按 Content-Type、BOM、<meta> 猜，最后按 UTF-8",
            ),
            ParamSchema::new("max_bytes", SchemaType::Int, false)
                .with_default(256 * 1024 * 1024)
                .with_description("响应体缓冲上限，超出即报错（避免把 10 GB 读进内存）"),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = require_map(&params)?;
        let url = require_str(map, "url")?;
        let method = parse_method(map.get("method").and_then(|v| v.as_str()).unwrap_or("GET"))?;
        let client = build_client(map)?;
        let mut builder = client.request(method, url);
        builder = with_query(builder, map)?;
        builder = with_headers(builder, map.get("headers"))?;
        builder = with_auth(builder, map)?;
        builder = with_body(builder, map, ctx).await?;
        let read = BodyRead::from_params(map)?;
        let resp = builder
            .send()
            .await
            .map_err(|e| ActionError::execution(format!("HTTP 请求失败: {e}")))?;
        response_to_value(resp, ctx, &read).await
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

fn with_query(
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

fn with_headers(
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

fn with_auth(
    mut builder: RequestBuilder,
    map: &BTreeMap<String, Value>,
) -> Result<RequestBuilder, ActionError> {
    if let Some(token) = map.get("token").and_then(|v| v.as_str())
        && !token.is_empty()
    {
        builder = builder.bearer_auth(token);
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

/// 请求体：json / form / multipart / body 四选一。
async fn with_body(
    mut builder: RequestBuilder,
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
) -> Result<RequestBuilder, ActionError> {
    let picked: Vec<&str> = ["json", "form", "multipart", "body"]
        .into_iter()
        .filter(|key| map.contains_key(*key))
        .collect();
    if picked.len() > 1 {
        return Err(ActionError::InvalidParams(format!(
            "{} 只能指定其一",
            picked.join(" / ")
        )));
    }

    match picked.first().copied() {
        Some("json") => {
            let json = map.get("json").expect("picked 里就有");
            builder = builder
                .json(&json.to_json())
                .header(CONTENT_TYPE, "application/json");
        }
        Some("form") => {
            if let Some(Value::Map(form)) = map.get("form") {
                let pairs: Vec<(String, String)> = form
                    .iter()
                    .map(|(k, v)| (k.clone(), value_to_string(v)))
                    .collect();
                builder = builder.form(&pairs);
            }
        }
        Some("multipart") => builder = with_multipart(builder, map, ctx).await?,
        Some("body") => {
            if let Some(body) = map.get("body") {
                // `as_bytes` 也认 IPC 往返后退化成的整数数组。
                builder = match as_bytes(body) {
                    Some(bytes) => builder.body(bytes),
                    None => builder.body(body.to_string()),
                };
            }
        }
        _ => {}
    }
    Ok(builder)
}

/// `multipart/form-data` 请求体。
///
/// 字段值要么是标量（按文本字段发），要么是一张「文件规格」表：
/// `{path, offset?, length?, filename?, content_type?}`。带 `offset` / `length` 时只发这一段字节，
/// 于是分片上传不必先把每一片写到临时文件里（`generate.chunks` 给的正是这两个数）。
async fn with_multipart(
    mut builder: RequestBuilder,
    map: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
) -> Result<RequestBuilder, ActionError> {
    let Some(Value::Map(fields)) = map.get("multipart") else {
        return Err(ActionError::InvalidParams(
            "multipart 需要一个字段表：字段名 → 值".into(),
        ));
    };
    let mut form = Form::new();
    for (name, value) in fields {
        match file_spec(value) {
            Some(spec) => form = form.part(name.clone(), file_part(name, spec, ctx).await?),
            None => form = form.text(name.clone(), value_to_string(value)),
        }
    }
    builder = builder.multipart(form);
    Ok(builder)
}

/// 是「文件规格」还是一行普通文本。
///
/// 判据就是有没有 `path`：`{"path": "a.bin"}` 是文件，`{"a": "b"}` 是文本字段。
fn file_spec(value: &Value) -> Option<&BTreeMap<String, Value>> {
    let Value::Map(map) = value else {
        return None;
    };
    map.contains_key("path").then_some(map)
}

/// 把一个文件规格变成 multipart 部件。
async fn file_part(
    field: &str,
    spec: &BTreeMap<String, Value>,
    ctx: &ExecutionContext,
) -> Result<Part, ActionError> {
    let path = confine_path(ctx, Path::new(&require_str(spec, "path")?))?;
    let (offset, length) = range_params(spec)?;
    let bytes = read_range(&path, offset, length, MAX_RANGE).await?;

    let mut part = Part::bytes(bytes);
    // 没给文件名就用源文件名：服务端通常按 filename 判断「这是文件部件」。
    let filename = opt_str(spec, "filename")
        .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()));
    if let Some(name) = filename {
        part = part.file_name(name);
    }
    if let Some(mime) = opt_str(spec, "content_type") {
        part = part.mime_str(&mime).map_err(|e| {
            ActionError::InvalidParams(format!("multipart.{field}.content_type 无效: {e}"))
        })?;
    }
    Ok(part)
}

async fn response_to_value(
    mut resp: reqwest::Response,
    ctx: &ExecutionContext,
    read: &BodyRead,
) -> Result<Value, ActionError> {
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
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = read_body(&mut resp, ctx, read, content_type.as_deref()).await?;
    let mut out = BTreeMap::new();
    out.insert("status".into(), Value::Int(status));
    out.insert("ok".into(), Value::Bool(ok));
    out.insert("url".into(), Value::Str(final_url));
    out.insert("headers".into(), Value::Map(headers_map));
    out.insert("body".into(), body);
    Ok(Value::Map(out))
}

/// 响应体怎么收：文本还是原始字节、按什么字符集、缓冲上限多少。
struct BodyRead {
    binary: bool,
    /// 显式字符集；`None` 时交给 [`detect_charset`] 猜。
    charset: Option<String>,
    max: u64,
}

impl BodyRead {
    fn from_params(map: &BTreeMap<String, Value>) -> Result<Self, ActionError> {
        let binary = match opt_str(map, "response")
            .unwrap_or_else(|| "text".into())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "" | "text" => false,
            "binary" | "bytes" => true,
            other => {
                return Err(ActionError::InvalidParams(format!(
                    "不支持的 response: {other}（text | binary）"
                )));
            }
        };
        let charset = opt_str(map, "encoding").filter(|s| !s.trim().is_empty());
        // 字节流没有「字符集」可言：与其静默忽略 `encoding`，不如当场说清。
        if binary && charset.is_some() {
            return Err(ActionError::InvalidParams(
                "response: binary 下 encoding 无意义（字节不解码）".into(),
            ));
        }
        Ok(Self {
            binary,
            charset,
            max: opt_i64(map, "max_bytes", MAX_RANGE as i64).max(0) as u64,
        })
    }
}

/// `<meta>` 字符集只扫开头这么多字节。
const META_SNIFF_BYTES: usize = 4096;

/// 逐块读响应体，顺手把已下载字节报上去。
///
/// 大文件下载是 HTTP 动作里唯一耗得住时间的地方：一次 `text()` 读完，界面上就只剩
/// 一个不动的 spinner。`Content-Length` 在就有总量（分块传输 / 解压后就没有，只报已读）。
///
/// `response: binary` 时原样返回 [`Value::Bytes`]：图片、压缩包、PDF存不进 `String`。
async fn read_body(
    resp: &mut reqwest::Response,
    ctx: &ExecutionContext,
    read: &BodyRead,
    content_type: Option<&str>,
) -> Result<Value, ActionError> {
    let total = resp.content_length().filter(|n| *n > 0);
    let mut body = Vec::new();
    loop {
        let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| ActionError::execution(format!("读取响应失败: {e}")))?
        else {
            break;
        };
        body.extend_from_slice(&chunk);
        if body.len() as u64 > read.max {
            return Err(ActionError::InvalidParams(format!(
                "响应体超过 {} 字节上限（可用 max_bytes 提高）",
                read.max
            )));
        }
        ctx.chunk(body.len() as u64, total, Unit::Bytes);
    }
    if read.binary {
        return Ok(Value::Bytes(body));
    }
    let charset = detect_charset(read.charset.as_deref(), content_type, &body);
    decode_text(&body, charset.as_deref()).map(Value::Str)
}

/// 猜响应体的字符集，按可信度从高到低：显式参数 > HTTP 头 > BOM > `<meta>`。
///
/// `<meta>` 那一步是给中文站准备的：`charset=gbk` 往往只写在页面里，头里没有，
/// 不猜的话整页就是乱码。
fn detect_charset(
    explicit: Option<&str>,
    content_type: Option<&str>,
    body: &[u8],
) -> Option<String> {
    if let Some(name) = explicit {
        return Some(name.to_string());
    }
    if let Some(name) = content_type.and_then(charset_of_content_type) {
        return Some(name);
    }
    if let Some((encoding, _)) = encoding_rs::Encoding::for_bom(body) {
        return Some(encoding.name().to_string());
    }
    charset_from_meta(&body[..body.len().min(META_SNIFF_BYTES)])
}

/// `text/html; charset=utf-8` → `utf-8`。
fn charset_of_content_type(content_type: &str) -> Option<String> {
    let lowered = content_type.to_ascii_lowercase();
    let at = lowered.find("charset=")? + "charset=".len();
    let name = take_charset_name(&lowered[at..]);
    (!name.is_empty()).then_some(name)
}

/// 从 `<meta charset=gbk>` 或 `<meta content="text/html; charset=gbk">` 里捞字符集。
fn charset_from_meta(head: &[u8]) -> Option<String> {
    let lowered = String::from_utf8_lossy(head).to_ascii_lowercase();
    let at = lowered.find("charset=")? + "charset=".len();
    let name = take_charset_name(&lowered[at..]);
    (!name.is_empty()).then_some(name)
}

/// 吃掉前导空白、引号，以及尾随的 `;` / `/`。
fn take_charset_name(rest: &str) -> String {
    rest.trim_start()
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

/// 按字符集解码；猜不出来时按 UTF-8（非法字节替换，与 reqwest 默认一致）。
fn decode_text(bytes: &[u8], charset: Option<&str>) -> Result<String, ActionError> {
    let Some(name) = charset else {
        return Ok(String::from_utf8_lossy(bytes).into_owned());
    };
    let encoding = encoding_rs::Encoding::for_label(name.as_bytes())
        .ok_or_else(|| ActionError::InvalidParams(format!("不认识的字符集: {name}")))?;
    let (text, _, _) = encoding.decode(bytes);
    Ok(text.into_owned())
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
    use crate::builtin::util::probe::Probe;
    use corex_core::{ExecutionContext, Observer};
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

    #[tokio::test]
    async fn rejects_multiple_body_sources() {
        let ctx = ExecutionContext::default();
        let mut m = BTreeMap::new();
        m.insert("json".into(), Value::Map(BTreeMap::new()));
        m.insert("body".into(), Value::Str("x".into()));
        let err = with_body(Client::new().get("http://example.com"), &m, &ctx)
            .await
            .expect_err("conflict");
        assert!(err.to_string().contains("只能指定其一"), "{err}");

        m.remove("body");
        m.insert("multipart".into(), Value::Map(BTreeMap::new()));
        let err = with_body(Client::new().get("http://example.com"), &m, &ctx)
            .await
            .expect_err("conflict");
        let text = err.to_string();
        assert!(
            text.contains("json") && text.contains("multipart"),
            "{text}"
        );
    }

    /// 分片上传靠的就是这一条：multipart 的文件部件只发 `[offset, offset + length)`。
    #[tokio::test]
    async fn multipart_sends_only_the_requested_range() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        // 前 26 个字节是大写字母、后 10 个是数字：切片后两边都认得出来。
        std::fs::write(&path, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]).into_owned();
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
            raw
        });

        let mut m = BTreeMap::new();
        m.insert("url".into(), Value::Str(format!("http://{addr}/chunk")));
        m.insert("method".into(), Value::Str("POST".into()));
        m.insert(
            "multipart".into(),
            Value::Map(BTreeMap::from([
                ("id".into(), Value::Str("sess-1".into())),
                ("index".into(), Value::Int(1)),
                (
                    "chunk".into(),
                    Value::Map(BTreeMap::from([
                        ("path".into(), Value::Str(path.to_string_lossy().into())),
                        ("offset".into(), Value::Int(20)),
                        ("length".into(), Value::Int(8)),
                        ("filename".into(), Value::Str("part-1.bin".into())),
                        (
                            "content_type".into(),
                            Value::Str("application/octet-stream".into()),
                        ),
                    ])),
                ),
            ])),
        );
        let mut ctx = ExecutionContext::default();
        let out = HttpSend.execute(Value::Map(m), &mut ctx).await.unwrap();
        assert_eq!(out.as_map().unwrap().get("ok"), Some(&Value::Bool(true)));

        let raw = server.await.unwrap();
        assert!(raw.contains("name=\"id\""), "{raw}");
        assert!(raw.contains("sess-1"), "{raw}");
        assert!(raw.contains("name=\"index\""), "{raw}");
        assert!(raw.contains("filename=\"part-1.bin\""), "{raw}");
        assert!(raw.contains("application/octet-stream"), "{raw}");
        // 只发了 offset 20 起的那 8 个字节：'U'..'Z' + '0'..'1'。
        assert!(raw.contains("UVWXYZ01"), "{raw}");
        assert!(!raw.contains("ABCDEFGH"), "整段都发出去了：{raw}");
    }

    /// 大文件下载要能看见字节在走：帧的总量取自 `Content-Length`。
    #[tokio::test]
    async fn reports_download_bytes() {
        const SIZE: usize = 64 * 1024;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = sock.read(&mut buf).await;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {SIZE}\r\nConnection: close\r\n\r\n"
            );
            let _ = sock.write_all(head.as_bytes()).await;
            // 分两段写：读方至少能拿到不止一块，进度就不止一帧。
            let _ = sock.write_all(&vec![b'a'; SIZE / 2]).await;
            let _ = sock.flush().await;
            let _ = tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let _ = sock.write_all(&vec![b'a'; SIZE / 2]).await;
        });

        let probe = Probe::bytes();
        let mut ctx = ExecutionContext::default();
        ctx.observer = Some(std::sync::Arc::clone(&probe) as std::sync::Arc<dyn Observer>);
        ctx.enter_step("fetch", "http.send");
        let mut m = BTreeMap::new();
        m.insert("url".into(), Value::Str(format!("http://{addr}/big")));
        let out = HttpSend.execute(Value::Map(m), &mut ctx).await.unwrap();
        let body = out.as_map().unwrap().get("body").unwrap();
        assert_eq!(body.as_str().unwrap().len(), SIZE);

        let total = SIZE as u64;
        let marks = probe.marks();
        assert!(!marks.is_empty(), "应当有分块帧");
        assert!(
            marks.iter().all(|(_, each)| *each == Some(total)),
            "总量取自 Content-Length：{marks:?}"
        );
        assert_eq!(marks.last(), Some(&(total, Some(total))), "{marks:?}");
        assert!(
            marks.windows(2).all(|w| w[0].0 < w[1].0),
            "帧应当单调递增：{marks:?}"
        );
    }

    /// 起一个只回答一次的服务端，响应体是**原始字节**、Content-Type 由调用方给。
    async fn serve_once_raw(content_type: &str, body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let content_type = content_type.to_string();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = sock.read(&mut buf).await.unwrap_or(0);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = sock.write_all(head.as_bytes()).await;
            let _ = sock.write_all(&body).await;
        });
        format!("http://{addr}/")
    }

    async fn get(map: &BTreeMap<String, Value>) -> Result<Value, ActionError> {
        let mut ctx = ExecutionContext::default();
        HttpSend.execute(Value::Map(map.clone()), &mut ctx).await
    }

    /// `response: binary` 原样给出字节：图片、压缩包过一趟 `String` 就废了。
    #[tokio::test]
    async fn binary_response_keeps_raw_bytes() {
        // 0x89 0x50 0x4E 0x47 是 PNG magic，不是合法 UTF-8。
        let png = vec![0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0x00];
        let url = serve_once_raw("image/png", png.clone()).await;
        let out = get(&BTreeMap::from([
            ("url".into(), Value::Str(url)),
            ("response".into(), Value::Str("binary".into())),
        ]))
        .await
        .expect("binary send");
        assert_eq!(out.as_map().unwrap().get("body"), Some(&Value::Bytes(png)));
    }

    /// 默认仍是字符串；GBK 页面按 Content-Type 里的 charset 解码，不是乱码。
    #[tokio::test]
    async fn text_response_follows_charset_header() {
        // 编码过的「分片」：UTF-8 解出来会是替换字符。
        let (gbk, _, _) = encoding_rs::GBK.encode("分片");
        let url = serve_once_raw("text/html; charset=gbk", gbk.into_owned()).await;
        let out = get(&BTreeMap::from([("url".into(), Value::Str(url))]))
            .await
            .expect("text send");
        assert_eq!(
            out.as_map().unwrap().get("body").unwrap().as_str(),
            Some("分片")
        );
    }

    /// 头里没写 charset 时从前 4 KiB 的 `<meta>` 里捞——中文站常只写在页面里。
    #[tokio::test]
    async fn meta_charset_is_sniffed_when_the_header_is_silent() {
        let (gbk, _, _) = encoding_rs::GBK.encode("<html><meta charset=gbk><p>分片</p></html>");
        let url = serve_once_raw("text/html", gbk.into_owned()).await;
        let out = get(&BTreeMap::from([("url".into(), Value::Str(url))]))
            .await
            .expect("meta sniff");
        let body = out.as_map().unwrap().get("body").unwrap().as_str().unwrap();
        assert!(body.contains("分片"), "{body}");
    }

    /// 缓冲上限是硬的：超大响应要报错，而不是把内存吃光。
    #[tokio::test]
    async fn response_over_max_bytes_is_rejected() {
        let url = serve_once_raw("application/octet-stream", vec![b'x'; 4096]).await;
        let err = get(&BTreeMap::from([
            ("url".into(), Value::Str(url)),
            ("max_bytes".into(), Value::Int(1024)),
        ]))
        .await
        .expect_err("超出上限");
        assert!(err.to_string().contains("max_bytes"), "{err}");
    }

    /// 字符集写错要报参数错，不要悄悄按 UTF-8 糊过去。
    #[tokio::test]
    async fn unknown_encoding_is_an_error() {
        let url = serve_once_raw("text/plain", b"hello".to_vec()).await;
        let err = get(&BTreeMap::from([
            ("url".into(), Value::Str(url)),
            ("encoding".into(), Value::Str("klingon".into())),
        ]))
        .await
        .expect_err("未知字符集");
        assert!(err.to_string().contains("klingon"), "{err}");
    }

    /// 字节流没有字符集可言：`response: binary` 下 `encoding` 当场报错。
    #[tokio::test]
    async fn binary_response_rejects_encoding() {
        let url = serve_once_raw("application/octet-stream", vec![0xFF]).await;
        let err = get(&BTreeMap::from([
            ("url".into(), Value::Str(url)),
            ("response".into(), Value::Str("binary".into())),
            ("encoding".into(), Value::Str("gbk".into())),
        ]))
        .await
        .expect_err("binary + encoding");
        assert!(err.to_string().contains("encoding"), "{err}");
    }

    /// `body` 也认 IPC 往返后退化成的整数数组。
    #[tokio::test]
    async fn body_accepts_integer_byte_array() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });

        let out = get(&BTreeMap::from([
            ("url".into(), Value::Str(format!("http://{addr}/"))),
            ("method".into(), Value::Str("POST".into())),
            (
                "body".into(),
                Value::Array(vec![Value::Int(65), Value::Int(66), Value::Int(67)]),
            ),
        ]))
        .await
        .expect("byte array body");
        assert_eq!(out.as_map().unwrap().get("ok"), Some(&Value::Bool(true)));
        assert!(
            server.await.unwrap().ends_with("ABC"),
            "体应该是三个原始字节"
        );
    }
}
