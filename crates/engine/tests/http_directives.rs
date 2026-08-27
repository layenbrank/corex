//! HTTP directive integration: mock server + file.write / json.parse chain.

use corex_core::{ExecutionContext, RuntimeConfig, Value};
use corex_engine::{Pipeline, Directive};
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
    assert!(text.contains("\"ok\":true") || text.contains("ok"), "body={text}");
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
        result.get_path("data.msg").and_then(|v| v.as_str()),
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
