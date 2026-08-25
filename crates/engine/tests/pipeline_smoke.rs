//! End-to-end smoke: resolver + template/file pipeline.

use corex_core::{ExecutionContext, RuntimeConfig, Value};
use corex_engine::{Pipeline, Resolver, Shortcut};
use corex_registry::ActionRegistry;
use std::sync::Arc;

#[tokio::test]
async fn hello_template_and_file_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("hello.txt");
    let path = out.to_string_lossy().replace('\\', "/");

    let yaml = format!(
        r#"
name: smoke
inputs:
  - name: who
    default: "corex"
variables:
  prefix: "Hi"
steps:
  - id: greet
    action: template.render
    params:
      template: "{{{{ prefix }}}}, {{{{ who }}}}!"
      context:
        prefix: "{{{{prefix}}}}"
        who: "{{{{input.who}}}}"
    save_to: message
  - id: write
    action: file.write
    params:
      path: "{path}"
      content: "{{{{message}}}}"
"#,
        path = path
    );

    let shortcut = Shortcut::from_yaml_str(&yaml).expect("parse shortcut");
    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    let pipeline = Pipeline::new(Arc::new(registry));

    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let result = pipeline.execute(&shortcut, ctx).await.expect("execute");

    assert!(out.exists(), "output file should exist");
    let text = std::fs::read_to_string(&out).unwrap();
    assert_eq!(text, "Hi, corex!");
    // last step returns File path
    match result {
        Value::File(p) => assert_eq!(p, out),
        other => panic!("expected File, got {other}"),
    }
}

#[tokio::test]
async fn resolver_variables() {
    let mut ctx = ExecutionContext::new(RuntimeConfig::default());
    ctx.set_variable("name", Value::Str("world".into()));
    let v = Resolver::resolve_string("hello {{name}}", &ctx).unwrap();
    assert_eq!(v.as_str(), Some("hello world"));
}
