//! watch 模块 smoke 测试

use cx::pipeline::config::{
    CONFIG_VERSION, PipelineConfig, PipelinesConfig, StepConfig, WatchConfig, validate_config,
};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn watch_pipeline_passes_validation() {
    let config = PipelinesConfig {
        version: CONFIG_VERSION,
        variables: HashMap::new(),
        pipelines: vec![PipelineConfig {
            id: "watched".into(),
            description: Some("dev watch".into()),
            schedule: None,
            watch: Some(WatchConfig {
                paths: vec![".".into()],
                includes: vec!["**/*.rs".into()],
                excludes: vec!["**/.git/**".into()],
                debounce_ms: 200,
                cooldown_ms: None,
            }),
            steps: vec![StepConfig {
                id: "scan_os".into(),
                module: "scan".into(),
                description: None,
                depends_on: vec![],
                when: None,
                retry: None,
                params: json!({"Os": {}}),
            }],
        }],
    };

    validate_config(&config).expect("watch pipeline should validate");
}
