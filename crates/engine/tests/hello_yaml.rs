//! 工作区级冒烟测试（可选；引擎测试也覆盖了）。

#[test]
fn hello_yaml_parses() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/directives/hello.yaml");
    // 从 engine 包目录跑时路径可能不同——再试一下仓库相对路径。
    let candidates = [
        path,
        std::path::PathBuf::from("examples/directives/hello.yaml"),
        std::path::PathBuf::from("../examples/directives/hello.yaml"),
        std::path::PathBuf::from("../../examples/directives/hello.yaml"),
    ];
    let yaml_path = candidates.into_iter().find(|p| p.exists());
    if let Some(p) = yaml_path {
        let s = corex_engine::Directive::from_yaml_file(&p).expect("parse hello.yaml");
        assert_eq!(s.name, "hello");
        assert!(!s.steps.is_empty());
    }
}
