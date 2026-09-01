//! Parallel partial-failure behavior with on_error abort vs continue.

use corex_core::{ExecutionContext, RuntimeConfig, Value};
use corex_engine::{Pipeline, Directive};
use corex_registry::ActionRegistry;
use std::sync::Arc;

fn registry() -> Arc<ActionRegistry> {
    let mut r = ActionRegistry::new();
    r.register_builtins();
    Arc::new(r)
}

#[tokio::test]
async fn parallel_abort_returns_err() {
    let yaml = r#"
name: parallel-abort
on_error: abort
steps:
  - id: fanout
    max_concurrency: 2
    parallel:
      - id: a
        action: template.render
        params:
          template: "A"
        save_to: a
      - id: b
        action: shell.run
        params:
          command: "corex-definitely-missing-cmd-xyz"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let err = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect_err("abort must surface the failing branch");
    let msg = err.to_string();
    assert!(
        !msg.is_empty(),
        "expected non-empty error from failed shell branch"
    );
}

#[tokio::test]
async fn parallel_abort_max1_same_as_concurrent() {
    let yaml = r#"
name: parallel-abort-max1
on_error: abort
steps:
  - id: fanout
    max_concurrency: 1
    parallel:
      - id: a
        action: template.render
        params:
          template: "A"
        save_to: a
      - id: b
        action: shell.run
        params:
          command: "corex-definitely-missing-cmd-xyz"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let err = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect_err("max_concurrency=1 must also abort");
    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn parallel_continue_null_for_failed_branch() {
    let yaml = r#"
name: parallel-continue
on_error: continue
steps:
  - id: fanout
    max_concurrency: 2
    parallel:
      - id: a
        action: template.render
        params:
          template: "A"
        save_to: a
      - id: b
        action: shell.run
        params:
          command: "corex-definitely-missing-cmd-xyz"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect("continue should return Ok with partial results");

    let list = match result {
        Value::List(items) => items,
        other => panic!("expected List, got {other}"),
    };
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].as_str(), Some("A"));
    assert!(
        matches!(list[1], Value::Null),
        "failed branch should be Null, got {:?}",
        list[1]
    );
}

#[tokio::test]
async fn parallel_permission_denied_not_swallowed_by_continue() {
    let yaml = r#"
name: parallel-perm
on_error: continue
permissions:
  notifications: true
steps:
  - id: fanout
    max_concurrency: 2
    parallel:
      - id: ok
        action: template.render
        params:
          template: "ok"
      - id: denied
        action: shell.run
        params:
          command: "true"
"#;
    let directive = Directive::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let err = pipeline
        .execute(&directive, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect_err("PermissionDenied in parallel must abort even with on_error=continue");
    assert!(
        err.is_permission_denied(),
        "expected permission_denied, got: {err}"
    );
}
