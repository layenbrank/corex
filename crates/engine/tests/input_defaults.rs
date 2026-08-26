//! Integration tests for directive input defaults.

use corex_core::{ExecutionContext, RuntimeConfig, Value};
use corex_engine::{Directive, Pipeline};
use corex_registry::ActionRegistry;
use std::sync::Arc;

#[tokio::test]
async fn optional_default_used_in_step_params() {
    let yaml = r#"
name: input-default-smoke
inputs:
  - name: path
    required: false
    default: "C:\\WeChat\\WeChat.exe"
steps:
  - id: echo
    action: template.render
    params:
      template: "{{input.path}}"
    save_to: out
"#;
    let directive = Directive::from_yaml_str(yaml).expect("parse");
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    let pipeline = Pipeline::new(Arc::new(reg));
    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let result = pipeline.execute(&directive, ctx).await.expect("execute");
    match result {
        Value::Str(s) => assert_eq!(s, "C:\\WeChat\\WeChat.exe"),
        other => panic!("expected str, got {other:?}"),
    }
}

#[tokio::test]
async fn empty_string_input_gets_default() {
    let yaml = r#"
name: input-default-empty
inputs:
  - name: path
    required: false
    default: "/fallback"
steps:
  - id: echo
    action: template.render
    params:
      template: "{{input.path}}"
"#;
    let directive = Directive::from_yaml_str(yaml).expect("parse");
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    let pipeline = Pipeline::new(Arc::new(reg));
    let mut input = std::collections::HashMap::new();
    input.insert("path".into(), Value::Str("".into()));
    let ctx = ExecutionContext::new(RuntimeConfig::default()).with_input(input);
    let result = pipeline.execute(&directive, ctx).await.expect("execute");
    assert_eq!(result.as_str(), Some("/fallback"));
}
