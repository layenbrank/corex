//! CLI 契约集成测试（exit code + JSON 输出）

use assert_cmd::Command;
use predicates::prelude::*;

fn corex() -> Command {
    Command::cargo_bin("corex").unwrap()
}

fn pipelines_yaml() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pipelines.yaml")
        .display()
        .to_string()
}

#[test]
fn watch_help_exits_zero() {
    corex().args(["watch", "--help"]).assert().success();
}

#[test]
fn pipeline_validate_human_exits_zero() {
    corex()
        .args(["pipeline", "--validate", "--config"])
        .arg(pipelines_yaml())
        .assert()
        .success();
}

#[test]
fn pipeline_validate_json_outputs_ok() {
    corex()
        .args(["pipeline", "--validate", "--format", "json", "--config"])
        .arg(pipelines_yaml())
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\":true"));
}

#[test]
fn pipeline_rejects_legacy_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.yaml");
    std::fs::write(
        &path,
        r#"version: 2
variables: {}
pipelines: []
"#,
    )
    .unwrap();

    corex()
        .args(["pipeline", "--validate", "--format", "json", "--config"])
        .arg(path)
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains("配置 version 必须为 3"));
}

#[test]
fn unknown_pipeline_id_exits_nonzero() {
    corex()
        .args([
            "pipeline",
            "--id",
            "does-not-exist",
            "--dry-run",
            "--config",
        ])
        .arg(pipelines_yaml())
        .assert()
        .failure();
}

#[test]
fn failed_pipeline_json_exits_runtime_code() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fail.yaml");
    std::fs::write(
        &path,
        r#"version: 3
variables: {}
pipelines:
  - id: fail-run
    steps:
      - id: bad
        module: copy
        params:
          from: /nonexistent/corex-test-path
          to: /nonexistent/corex-test-dest
          empty: false
          includes: []
          excludes: []
"#,
    )
    .unwrap();

    corex()
        .args([
            "pipeline",
            "--id",
            "fail-run",
            "--format",
            "json",
            "--config",
        ])
        .arg(&path)
        .assert()
        .failure()
        .code(3)
        .stdout(predicate::str::contains("\"status\":\"failed\""))
        .stdout(predicate::str::contains("\"pipeline_id\":\"fail-run\""));
}
