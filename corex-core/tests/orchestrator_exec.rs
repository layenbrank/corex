//! exec 模块 Pipeline orchestrator 集成测试

use cx::pipeline::config::{PipelineConfig, StepConfig};
use cx::pipeline::context::PipelineContext;
use cx::pipeline::orchestrator::run_pipeline;
use cx::pipeline::report::RunStatus;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn write_ps1(dir: &PathBuf, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    path
}

#[test]
fn pipeline_exec_step_returns_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let out_file = dir.path().join("artifact.json");
    let out_str = out_file.display().to_string().replace('\\', "/");

    let script = write_ps1(
        &dir.path().to_path_buf(),
        "pipeline_ok.ps1",
        &format!(
            r#"
@{{ ok = $true }} | ConvertTo-Json -Compress | Set-Content "{out_str}"
$result = @{{
    path = "{out_str}"
    data = @{{ status = "ok" }}
}}
Write-Output ($result | ConvertTo-Json -Compress)
"#
        ),
    );

    let pipeline = PipelineConfig {
        id: "exec-test".into(),
        description: None,
        schedule: None,
        watch: None,
        steps: vec![StepConfig {
            id: "run_script".into(),
            module: "exec".into(),
            action: Some("run".into()),
            params: json!({
                "script": script.display().to_string(),
                "args": [],
                "capture": "json"
            }),
            ..Default::default()
        }],
    };

    let mut ctx = PipelineContext::new();
    let report = run_pipeline(&pipeline, &mut ctx).expect("pipeline report");
    assert_eq!(report.status, RunStatus::Success);
    assert_eq!(report.steps.len(), 1);

    let artifact = ctx
        .step_artifacts
        .get("run_script")
        .expect("step artifact");
    assert!(artifact.path.is_some());
    assert_eq!(
        artifact.data.get("status").and_then(|v| v.as_str()),
        Some("ok")
    );
}
