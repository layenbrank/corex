//! Workspace-level smoke test (optional; also covered by engine tests).

#[test]
fn hello_yaml_parses() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/directives/hello.yaml");
    // When run from engine package, path may differ — try repo-relative.
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
