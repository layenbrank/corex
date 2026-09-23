//! 指令输入默认值的集成测试。

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

/// 默认值可以引用**前面**声明的输入：输入按声明顺序填充，后面的默认值拿得到前面的值。
/// （起步指令 `dir-backup` 的 archive 默认值就靠这条。）
#[tokio::test]
async fn default_may_reference_earlier_input() {
    let yaml = r#"
name: input-default-chain
inputs:
  - name: source
    required: true
  - name: archive
    required: false
    default: "{{input.source}}.zip"
steps:
  - id: echo
    action: template.render
    params:
      template: "{{input.source}} -> {{input.archive}}"
    save_to: out
"#;
    let directive = Directive::from_yaml_str(yaml).expect("parse");
    let mut reg = ActionRegistry::new();
    reg.register_builtins();
    let pipeline = Pipeline::new(Arc::new(reg));
    let mut input = std::collections::HashMap::new();
    input.insert("source".into(), Value::Str(r"D:\notes".into()));
    let ctx = ExecutionContext::new(RuntimeConfig::default()).with_input(input);
    let result = pipeline.execute(&directive, ctx).await.expect("execute");
    assert_eq!(result.as_str(), Some(r"D:\notes -> D:\notes.zip"));
}
