//! Pipeline orchestrator 失败报告测试

use cx::pipeline::config::{PipelineConfig, StepConfig};
use cx::pipeline::context::PipelineContext;
use cx::pipeline::orchestrator::run_pipeline;
use cx::pipeline::report::RunStatus;
use serde_json::json;

#[test]
fn failed_pipeline_returns_run_report() {
    let pipeline = PipelineConfig {
        id: "fail-test".into(),
        description: None,
        schedule: None,
        watch: None,
        steps: vec![StepConfig {
            id: "bad".into(),
            module: "copy".into(),
            description: None,
            depends_on: vec![],
            when: None,
            retry: None,
            params: json!({
                "from": "/nonexistent/path/xyz",
                "to": "/also/nonexistent/xyz",
                "empty": false,
                "includes": [],
                "excludes": []
            }),
        }],
    };

    let mut ctx = PipelineContext::new();
    let report = run_pipeline(&pipeline, &mut ctx).expect("should return report");
    assert_eq!(report.status, RunStatus::Failed);
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].id, "bad");
}
