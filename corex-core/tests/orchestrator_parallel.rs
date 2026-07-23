//! 并行层 artifact 合并与 steps 顺序测试

use cx::pipeline::config::{PipelineConfig, StepConfig};
use cx::pipeline::context::PipelineContext;
use cx::pipeline::orchestrator::run_pipeline;
use cx::pipeline::report::{RunStatus, StepStatus};
use serde_json::json;

fn uuid_step(id: &str, deps: Vec<&str>) -> StepConfig {
    StepConfig {
        id: id.into(),
        module: "generate".into(),
        action: Some("uuid".into()),
        depends_on: deps.into_iter().map(str::to_string).collect(),
        params: json!({ "count": 1, "uppercase": false }),
        ..Default::default()
    }
}

#[test]
fn parallel_layer_merges_artifacts_into_context() {
    let pipeline = PipelineConfig {
        id: "fork".into(),
        description: None,
        schedule: None,
        watch: None,
        steps: vec![
            uuid_step("root", vec![]),
            uuid_step("left", vec!["root"]),
            uuid_step("right", vec!["root"]),
        ],
    };

    let mut ctx = PipelineContext::new();
    let report = run_pipeline(&pipeline, &mut ctx).expect("pipeline should complete");

    assert_eq!(report.status, RunStatus::Success);
    assert_eq!(report.steps.len(), 3);
    assert!(report.steps.iter().all(|s| s.status == StepStatus::Success));

    assert!(ctx.step_artifacts.contains_key("root"));
    assert!(ctx.step_artifacts.contains_key("left"));
    assert!(ctx.step_artifacts.contains_key("right"));

    // 并行层 steps 完成顺序非确定性；按 id 排序后断言
    let mut parallel_ids: Vec<String> = report
        .steps
        .iter()
        .filter(|s| s.id == "left" || s.id == "right")
        .map(|s| s.id.clone())
        .collect();
    parallel_ids.sort();
    assert_eq!(parallel_ids, vec!["left", "right"]);
}
