//! Permissions gating, on_error skip/continue, and step timeout.

use corex_core::{ExecutionContext, RuntimeConfig};
use corex_engine::{Pipeline, Shortcut};
use corex_registry::ActionRegistry;
use std::sync::Arc;

fn registry() -> Arc<ActionRegistry> {
    let mut r = ActionRegistry::new();
    r.register_builtins();
    Arc::new(r)
}

#[tokio::test]
async fn filesystem_only_denies_shell_run() {
    let yaml = r#"
name: perm-deny-shell
permissions:
  filesystem: true
steps:
  - id: sh
    action: shell.run
    params:
      command: "true"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let err = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect_err("shell.run must be denied");
    let msg = err.to_string();
    assert!(
        msg.contains("PermissionDenied") || msg.contains("权限") || msg.contains("未声明权限"),
        "expected permission error, got: {msg}"
    );
}

#[tokio::test]
async fn default_permissions_allow_template_render() {
    let yaml = r#"
name: perm-default-template
steps:
  - id: t
    action: template.render
    params:
      template: "ok"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect("template.render should succeed under unrestricted defaults");
    assert_eq!(result.as_str(), Some("ok"));
}

#[tokio::test]
async fn shell_true_allows_template_none_kind() {
    let yaml = r#"
name: perm-shell-template
permissions:
  shell: true
steps:
  - id: t
    action: template.render
    params:
      template: "none-kind"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect("None-kind actions are allowed when any permission is declared");
    assert_eq!(result.as_str(), Some("none-kind"));
}

#[tokio::test]
async fn network_only_denies_shell_before_execute() {
    let yaml = r#"
name: perm-network-deny-shell
permissions:
  network: true
steps:
  - id: sh
    action: shell.run
    params:
      command: "true"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let err = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect_err("shell must be denied when only network is declared");
    let msg = err.to_string();
    assert!(
        msg.contains("权限") || msg.contains("未声明权限") || msg.contains("PermissionDenied"),
        "expected permission error, got: {msg}"
    );
}

#[tokio::test]
async fn on_error_continue_returns_null() {
    let yaml = r#"
name: on-error-continue
on_error: continue
steps:
  - id: boom
    action: shell.run
    params:
      command: "corex-definitely-missing-cmd-xyz"
  - id: after
    action: template.render
    params:
      template: "survived"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect("continue should not abort the pipeline");
    assert_eq!(result.as_str(), Some("survived"));
}

#[tokio::test]
async fn on_error_skip_does_not_abort() {
    let yaml = r#"
name: on-error-skip
on_error: skip
steps:
  - id: boom
    action: shell.run
    params:
      command: "corex-definitely-missing-cmd-xyz"
  - id: after
    action: template.render
    params:
      template: "after-skip"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .expect("skip should not abort the pipeline");
    assert_eq!(result.as_str(), Some("after-skip"));
}

#[tokio::test]
#[cfg(unix)]
async fn step_timeout_aborts_long_shell() {
    let yaml = r#"
name: step-timeout
steps:
  - id: slow
    action: shell.run
    params:
      command: "sleep"
      args: ["5"]
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let mut cfg = RuntimeConfig::default();
    cfg.step_timeout_secs = 1;
    let err = pipeline
        .execute(&shortcut, ExecutionContext::new(cfg))
        .await
        .expect_err("sleep 5 with 1s timeout must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("超时") || msg.to_lowercase().contains("timeout"),
        "expected timeout error, got: {msg}"
    );
}
