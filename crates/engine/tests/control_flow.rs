//! Control-flow tests: if / repeat / parallel.

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
async fn if_then_branch() {
    let yaml = r#"
name: if-then
variables:
  flag: true
steps:
  - id: branch
    if:
      eq: ["{{flag}}", true]
    then:
      - id: write_yes
        action: template.render
        params:
          template: "yes"
    else:
      - id: write_no
        action: template.render
        params:
          template: "no"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("yes"));
}

#[tokio::test]
async fn if_else_branch() {
    let yaml = r#"
name: if-else
variables:
  flag: false
steps:
  - id: branch
    if:
      eq: ["{{flag}}", true]
    then:
      - id: write_yes
        action: template.render
        params:
          template: "yes"
    else:
      - id: write_no
        action: template.render
        params:
          template: "no"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("no"));
}

#[tokio::test]
async fn repeat_count() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("count.txt");
    let path = out.to_string_lossy().replace('\\', "/");

    let yaml = format!(
        r#"
name: repeat-count
steps:
  - id: loop
    repeat:
      count: 3
      as: i
    steps:
      - id: append
        action: file.write
        params:
          path: "{path}"
          content: "n={{{{i}}}}"
"#
    );

    let shortcut = Shortcut::from_yaml_str(&yaml).unwrap();
    // Debug: ensure as_var parsed
    match &shortcut.steps[0] {
        corex_engine::Step::Repeat(r) => assert_eq!(r.repeat.as_var, "i"),
        other => panic!("expected repeat, got {other:?}"),
    }

    let pipeline = Pipeline::new(registry());
    pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert_eq!(text, "n=2");
}

#[tokio::test]
async fn parallel_merges_step_outputs() {
    let yaml = r#"
name: parallel-merge
steps:
  - id: fanout
    max_concurrency: 2
    parallel:
      - id: a
        action: template.render
        params:
          template: "A"
        save_to: va
      - id: b
        action: template.render
        params:
          template: "B"
        save_to: vb
  - id: join
    action: template.render
    params:
      template: "{{va}}-{{vb}}"
      context:
        va: "{{va}}"
        vb: "{{vb}}"
"#;
    let shortcut = Shortcut::from_yaml_str(yaml).unwrap();
    let pipeline = Pipeline::new(registry());
    let result = pipeline
        .execute(&shortcut, ExecutionContext::new(RuntimeConfig::default()))
        .await
        .unwrap();
    assert_eq!(result.as_str(), Some("A-B"));
}
