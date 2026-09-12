//! HTTP 指令集成：mock server + file.write / json.parse 链路。

use corex_core::{ExecutionContext, RuntimeConfig, Value};
use corex_engine::{Directive, Pipeline};
use corex_registry::ActionRegistry;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn serve_one_json(body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let body = body.to_string();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = sock.read(&mut buf).await;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = sock.write_all(resp.as_bytes()).await;
    });
    format!("http://{addr}/")
}

fn registry() -> Arc<ActionRegistry> {
    let mut r = ActionRegistry::new();
    r.register_builtins();
    Arc::new(r)
}

#[tokio::test]
async fn http_save_body_directive() {
    let url = serve_one_json(r#"{"ok":true,"n":42}"#).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("body.txt");
    let out_s = out.to_string_lossy().replace('\\', "/");

    let yaml = format!(
        r#"
name: http-save-body-test
permissions:
  network: true
  filesystem: true
steps:
  - id: fetch
    action: http.send
    params:
      url: "{url}"
      method: GET
    save_to: response
  - id: save
    action: file.write
    params:
      path: "{out_s}"
      content: "{{{{response.body}}}}"
      mode: overwrite
"#
    );
    let directive = Directive::from_yaml_str(&yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let ctx = ExecutionContext::new(RuntimeConfig::default());
    pipeline.execute(&directive, ctx).await.unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.contains("\"ok\":true") || text.contains("ok"),
        "body={text}"
    );
}

#[tokio::test]
async fn http_extract_parse_directive() {
    let url = serve_one_json(r#"{"data":{"msg":"hi"}}"#).await;
    let yaml = format!(
        r#"
name: http-extract-test
permissions:
  network: true
steps:
  - id: fetch
    action: http.send
    params:
      url: "{url}"
    save_to: response
  - id: parsed
    action: codec.json.parse
    params:
      text: "{{{{response.body}}}}"
    save_to: data
"#
    );
    let directive = Directive::from_yaml_str(&yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(
        result.find_path("data.msg").and_then(|v| v.as_str()),
        Some("hi")
    );
}

#[tokio::test]
async fn audit_records_action_id() {
    use corex_engine::ExecutionAudit;
    let dir = tempfile::tempdir().unwrap();
    let audit = ExecutionAudit::under_data_dir(dir.path()).unwrap();
    let yaml = r#"
name: audit-demo
steps:
  - id: t
    action: template.render
    params:
      template: "ok"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry()).with_audit(audit.clone());
    pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    let entries = audit.read_all().unwrap();
    assert!(!entries.is_empty());
    assert_eq!(entries[0].action_id, "template.render");
    assert_eq!(entries[0].name, "audit-demo");
    assert!(entries[0].ok);
    let _ = HashMap::<String, Value>::new();
}

/// 回一个原始字节的响应，Content-Type 由调用方给。
async fn serve_one_raw(content_type: &str, body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let content_type = content_type.to_string();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = sock.read(&mut buf).await;
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = sock.write_all(head.as_bytes()).await;
        let _ = sock.write_all(&body).await;
    });
    format!("http://{addr}/")
}

/// 下载链：`http.send`（`response: binary`）→ `file.write`。
///
/// 关键在于占位符的类型：`content: "{{got.body}}"` 整个就是一个 `{{ }}`，
/// 解析器因此保留 `Bytes` 而不会先转成字符串——否则这几字节就废了。
#[tokio::test]
async fn binary_download_roundtrips_through_file_write() {
    // 0x89 0x50 0x4E 0x47 是 PNG magic，不是合法 UTF-8。
    let png = vec![0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0x00];
    let url = serve_one_raw("image/png", png.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir
        .path()
        .join("pic.png")
        .to_string_lossy()
        .replace('\\', "/");

    let yaml = format!(
        r#"
name: http-binary-download
permissions:
  network: true
  filesystem: true
steps:
  - id: get
    action: http.send
    params:
      url: "{url}"
      response: binary
    save_to: got
  - id: store
    action: file.write
    params:
      path: "{out}"
      content: "{{{{got.body}}}}"
"#
    );
    let directive = Directive::from_yaml_str(&yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), png);
}

/// 爬取链：`http.send`（文本）→ `html.select` → 写出去。
#[tokio::test]
async fn html_select_extracts_from_a_fetched_page() {
    let page = r#"<!doctype html><html><head><title>T &amp; T</title></head>
<body><h1 class="t">Hello</h1><a href="/a">A</a></body></html>"#;
    let url = serve_one_raw("text/html; charset=utf-8", page.as_bytes().to_vec()).await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir
        .path()
        .join("t.txt")
        .to_string_lossy()
        .replace('\\', "/");

    let yaml = format!(
        r#"
name: http-html-extract
permissions:
  network: true
  filesystem: true
steps:
  - id: get
    action: http.send
    params:
      url: "{url}"
    save_to: page
  - id: title
    action: html.select
    params:
      html: "{{{{page.body}}}}"
      selector: "title"
      all: false
    save_to: title
  - id: links
    action: html.links
    params:
      html: "{{{{page.body}}}}"
      base: "{{{{page.url}}}}"
    save_to: links
  - id: save
    action: file.write
    params:
      path: "{out}"
      content: "{{{{title.value}}}}|{{{{links.count}}}}|{{{{links.value}}}}"
"#
    );
    let directive = Directive::from_yaml_str(&yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    let parts: Vec<&str> = text.split('|').collect();
    assert_eq!(parts.len(), 3, "{text}");
    // 实体解码（`&amp;`）生效。
    assert_eq!(parts[0], "T & T", "{text}");
    assert_eq!(parts[1], "1", "{text}");
    // 相对链接按 base 补成绝对链接：主机是本地 mock，端口不固定，只看形状。
    assert!(parts[2].starts_with("http://127.0.0.1:"), "{text}");
    assert!(parts[2].ends_with("/a"), "{text}");
}
