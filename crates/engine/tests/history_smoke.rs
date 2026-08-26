//! History recording smoke test.

use corex_core::{ExecutionContext, RuntimeConfig};
use corex_engine::{ExecutionHistory, Pipeline, Directive};
use corex_registry::ActionRegistry;
use std::sync::Arc;

#[tokio::test]
async fn records_success_to_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let hist = ExecutionHistory::under_data_dir(dir.path()).unwrap();

    let yaml = r#"
name: hist-smoke
steps:
  - id: greet
    action: template.render
    params:
      template: "ok"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    let pipeline = Pipeline::new(Arc::new(registry)).with_history(hist.clone());

    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let result = pipeline.execute(&directive, ctx).await.unwrap();
    assert_eq!(result.as_str(), Some("ok"));

    let entries = hist.read_all().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].directive, "hist-smoke");
    assert!(entries[0].ok);
    assert!(entries[0].error.is_none());
    assert!(entries[0].duration_ms < 60_000);
}

#[tokio::test]
async fn records_failure_to_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let hist = ExecutionHistory::under_data_dir(dir.path()).unwrap();

    let yaml = r#"
name: hist-fail
steps:
  - id: missing
    action: does.not.exist
    params: {}
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let mut registry = ActionRegistry::new();
    registry.register_builtins();
    let pipeline = Pipeline::new(Arc::new(registry)).with_history(hist.clone());

    let ctx = ExecutionContext::new(RuntimeConfig::default());
    let err = pipeline.execute(&directive, ctx).await.unwrap_err();
    assert!(err.to_string().contains("does.not.exist"));

    let entries = hist.read_all().unwrap();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].ok);
    assert!(entries[0]
        .error
        .as_ref()
        .unwrap()
        .contains("does.not.exist"));
}
