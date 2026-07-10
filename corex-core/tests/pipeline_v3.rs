//! Pipeline v3 配置校验集成测试

use cx::pipeline::config::{CONFIG_VERSION, load_config, validate_config};

#[test]
fn pipelines_yaml_v3_validates() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../pipelines.yaml");
    if !path.exists() {
        return;
    }
    let config = load_config(&path).expect("load pipelines.yaml");
    assert_eq!(config.version, CONFIG_VERSION);
    validate_config(&config).expect("validate pipelines.yaml");
}
